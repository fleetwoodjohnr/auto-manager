#!/usr/bin/env python3
"""Install each release package the way a person would, run the installed app, rehearse an
in-app update, and uninstall. Adapted from Simple Connect's package-smoke.py.

Runs only on disposable native CI runners, with an isolated data folder. Per platform:
  Windows  NSIS silent install -> PE architecture, bundled OCR models -> self-test, launch ->
           updater rejects a tampered package, installs the signed one -> self-test -> uninstall
  macOS    DMG mount and copy -> codesign verify, architecture, bundled OCR models -> self-test,
           launch -> update rehearsal (re-verified signature) -> self-test
  Linux    .deb installed and removed on clean Ubuntu, .rpm on clean Fedora (self-test each);
           AppImage launched under Xvfb -> update rehearsal
"""
import argparse
from contextlib import contextmanager
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import threading
import time

OCR_MODELS = ('text-detection.onnx', 'text-recognition.onnx')


def run(args, log, *, cwd=None, env=None, timeout=120):
    print('Running:', ' '.join(map(str, args)), flush=True)
    # A ShellExecute/NSIS descendant can inherit a PIPE after the example exits. On Windows
    # subprocess.run's timeout then waits forever for EOF; a log file has no pipe reader.
    with log.open('ab') as output:
        start = output.tell()
        process = subprocess.Popen(list(map(str, args)), cwd=cwd, env=env, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=os.name != 'nt')
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired as error:
            output.write(f'\nCommand timed out after {timeout}s; terminating process tree\n'.encode())
            output.flush()
            if os.name == 'nt':
                subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'], stdout=output,
                               stderr=subprocess.STDOUT, timeout=30)
            else:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            if process.poll() is None:
                process.kill()
            process.wait(timeout=15)
            raise RuntimeError(f'{args[0]} timed out after {timeout}s; see {log}') from error
        output.flush()
        end = os.fstat(output.fileno()).st_size
    with log.open('rb') as captured:
        captured.seek(start)
        text = captured.read(max(0, end - start)).decode(errors='replace')
    if code:
        print(text[-4000:], flush=True)
        raise RuntimeError(f'{args[0]} exited {code}; see {log}')
    return text


def one(bundle, pattern):
    matches = list(bundle.rglob(pattern))
    if len(matches) != 1:
        raise RuntimeError(f'Expected one {pattern}, found {len(matches)} in {bundle}')
    return matches[0].resolve()


def isolated_env(profile, extra=None):
    env = os.environ.copy()
    env['AUTO_MANAGER_DATA_DIR'] = str(profile / 'data')
    env['HOME'] = str(profile)
    env['XDG_CONFIG_HOME'] = str(profile / 'config')
    env['XDG_DATA_HOME'] = str(profile / 'xdg-data')
    env['XDG_STATE_HOME'] = str(profile / 'state')
    env['XDG_RUNTIME_DIR'] = str(profile / 'runtime')
    env['WEBKIT_DISABLE_DMABUF_RENDERER'] = '1'
    env.update(extra or {})
    for d in (profile, profile / 'runtime'):
        d.mkdir(mode=0o700, parents=True, exist_ok=True)
    return env


def self_test(command, profile, logs, label):
    out = run([*command, '--self-test'], logs / 'self-test.log', env=isolated_env(profile))
    line = out.strip().splitlines()[-1] if out.strip() else ''
    try:
        result = json.loads(line)
    except json.JSONDecodeError as error:
        raise RuntimeError(f'{label}: self-test printed no result: {out!r}') from error
    if result.get('integrity') != 'ok':
        raise RuntimeError(f'{label}: self-test failed: {result}')
    print(f'{label}: self-test {result}', flush=True)
    return result


def check_models(directory, label):
    for name in OCR_MODELS:
        model = directory / name
        if not model.is_file() or model.stat().st_size < 1_000_000:
            raise RuntimeError(f'{label}: bundled text-recognition model missing: {model}')
    print(f'{label}: OCR models present in {directory}', flush=True)


def launch(command, profile, logs, *, extra_env=None):
    """The installed app must start its window and stay up for 10 seconds."""
    env = isolated_env(profile, extra_env)
    log = logs / 'application.log'
    with log.open('ab') as output:
        proc = subprocess.Popen(list(map(str, command)), env=env, stdout=output, stderr=subprocess.STDOUT,
                                start_new_session=os.name != 'nt')
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if proc.poll() is not None:
                    raise RuntimeError(f'Application exited early ({proc.returncode}); see {log}')
                time.sleep(0.2)
            print('Installed application stayed running for 10 seconds', flush=True)
        finally:
            if os.name == 'nt':
                subprocess.run(['taskkill', '/PID', str(proc.pid), '/T', '/F'], stdout=output, stderr=output)
            else:
                try:
                    os.killpg(proc.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
            try:
                proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                if os.name != 'nt':
                    os.killpg(proc.pid, signal.SIGKILL)
                else:
                    proc.kill()
                proc.wait(timeout=10)
    if not (profile / 'data' / 'auto-manager.db').is_file():
        raise RuntimeError(f'The launched app did not create its database; see {log}')


def digest(path):
    checksum = hashlib.sha256()
    with Path(path).open('rb') as source:
        while chunk := source.read(1024 * 1024):
            checksum.update(chunk)
    return checksum.hexdigest()


def installed_resource_hashes(directory):
    # NSIS regenerates the uninstaller; its bytes need not match the first run.
    return {str(file.relative_to(directory)): digest(file)
            for file in directory.rglob('*') if file.is_file() and file.name.lower() != 'uninstall.exe'}


def wait_installed_resources(directory, expected, *, timeout=180, stable_seconds=2):
    """The updater exits before NSIS; don't race its remaining writes."""
    deadline = time.monotonic() + timeout
    stable_since = None
    while time.monotonic() < deadline:
        try:
            matches = (directory / 'uninstall.exe').is_file() and all(
                digest(directory / relative) == checksum for relative, checksum in expected.items())
        except (FileNotFoundError, PermissionError):
            matches = False
        if matches:
            stable_since = stable_since or time.monotonic()
            if time.monotonic() - stable_since >= stable_seconds:
                return
        else:
            stable_since = None
        time.sleep(0.2)
    raise RuntimeError('NSIS update did not finish restoring the executable and bundled resources')


def wait_uninstalled(paths, *, timeout=180):
    """Wait for NSIS's self-copy child, not just its short-lived launcher."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if all(not path.exists() for path in paths):
                return
        except PermissionError:
            pass
        time.sleep(0.2)
    remaining = ', '.join(str(path) for path in paths if path.exists())
    raise RuntimeError(f'NSIS uninstall did not complete; files remain: {remaining}')


@contextmanager
def update_fixture(artifact, target):
    """A local stand-in for GitHub's latest.json and download; the first serving is corrupted."""
    signature = Path(f'{artifact}.sig').read_text().strip()
    version = json.loads(Path('package.json').read_text())['version']
    state = {'tamper': True}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            if self.path == '/latest.json':
                manifest = {'version': version, 'platforms': {target: {
                    'url': f'http://127.0.0.1:{self.server.server_port}/package',
                    'signature': signature,
                }}}
                body = json.dumps(manifest).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            elif self.path == '/package':
                self.send_response(200)
                self.send_header('Content-Length', str(artifact.stat().st_size))
                self.end_headers()
                with artifact.open('rb') as source:
                    first = True
                    while chunk := source.read(1024 * 1024):
                        if first and state['tamper']:
                            chunk = bytes([chunk[0] ^ 1]) + chunk[1:]
                        first = False
                        self.wfile.write(chunk)
            else:
                self.send_error(404)

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f'http://127.0.0.1:{server.server_port}/latest.json', state
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=10)


def rehearse_update(harness, installed, artifact, target, profile, logs, *, installed_app=None):
    """Run the real updater against corrupt and genuine local bytes. The harness reports itself
    as version 0.0.9, so this release is offered as an update."""
    database = profile / 'data' / 'auto-manager.db'
    data_hash = digest(database)
    expected_hash = digest(installed)
    windows_resources = installed_resource_hashes(installed.parent) if os.name == 'nt' else None
    if sys.platform == 'linux':
        # A distinct "installed old" AppImage, never the distributable candidate.
        with installed.open('ab') as old_image:
            old_image.write(b'\ninstalled-0.0.9-rehearsal\n')
        command = ['xvfb-run', '-a', 'dbus-run-session', '--', harness]
    else:
        shutil.copy2(harness, installed)
        if installed_app:
            run(['codesign', '--force', '--deep', '--sign', '-', installed_app], logs / 'update-rehearsal.log')
        command = [installed]
    old_hash = digest(installed)
    if old_hash == expected_hash:
        raise RuntimeError('Installed older rehearsal binary must differ from candidate')
    env = isolated_env(profile, {'APPIMAGE': str(installed)} if sys.platform == 'linux' else None)
    with update_fixture(artifact, target) as (feed, state):
        result = run([*command, 'reject-tamper', feed, installed, target], logs / 'update-rehearsal.log', env=env, timeout=240)
        if 'Tampered download rejected by native minisign verification' not in result:
            raise RuntimeError('Native updater did not confirm signature rejection')
        if digest(installed) != old_hash or digest(database) != data_hash:
            raise RuntimeError('Tampered update changed the installation or the data')
        state['tamper'] = False
        run([*command, 'install', feed, installed, target], logs / 'update-rehearsal.log', env=env, timeout=240)
        # Windows' updater exits before its NSIS installer finishes.
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            try:
                if digest(installed) == expected_hash:
                    break
            except (FileNotFoundError, PermissionError):
                pass
            time.sleep(0.2)
        else:
            raise RuntimeError('Native updater did not install the signed package')
    if windows_resources is not None:
        wait_installed_resources(installed.parent, windows_resources)
    if digest(database) != data_hash:
        raise RuntimeError('The data changed during the update')
    if installed_app:
        run(['codesign', '--verify', '--deep', '--strict', installed_app], logs / 'update-rehearsal.log')
    print(f'{target}: updater refused a tampered package, installed the signed one, data kept', flush=True)


def verify_windows_manifest(binary, log):
    """Read the harness's embedded manifest without executing its imports."""
    import ctypes
    from ctypes import wintypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
    kernel.LoadLibraryExW.restype = wintypes.HMODULE
    kernel.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p]
    kernel.FindResourceW.restype = wintypes.HANDLE
    kernel.SizeofResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
    kernel.SizeofResource.restype = wintypes.DWORD
    kernel.LoadResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
    kernel.LoadResource.restype = wintypes.HANDLE
    kernel.LockResource.argtypes = [wintypes.HANDLE]
    kernel.LockResource.restype = ctypes.c_void_p
    kernel.FreeLibrary.argtypes = [wintypes.HMODULE]
    kernel.FreeLibrary.restype = wintypes.BOOL
    module = kernel.LoadLibraryExW(str(binary), None, 2)  # LOAD_LIBRARY_AS_DATAFILE
    if not module:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        resource = kernel.FindResourceW(module, ctypes.c_void_p(1), ctypes.c_void_p(24))  # RT_MANIFEST
        if not resource:
            raise RuntimeError('Updater harness has no embedded application manifest')
        size = kernel.SizeofResource(module, resource)
        loaded = kernel.LoadResource(module, resource)
        pointer = kernel.LockResource(loaded) if loaded else None
        if not pointer or not size:
            raise RuntimeError('Could not read the updater harness manifest')
        manifest = ctypes.string_at(pointer, size)
        log.write_bytes(manifest)
        if b'Microsoft.Windows.Common-Controls' not in manifest or b'version="6.0.0.0"' not in manifest:
            raise RuntimeError('Updater harness must activate Common Controls v6')
    finally:
        kernel.FreeLibrary(module)


def windows(bundle, arch, work, logs, harness):
    verify_windows_manifest(harness, logs / 'updater-harness.manifest')
    installer = one(bundle, '*-setup.exe')
    destination = work / 'installed'
    # NSIS /D must be last and unquoted; refuse a path that would need quoting.
    if ' ' in str(destination):
        raise RuntimeError('NSIS smoke install requires a space-free temporary root')
    run([installer, '/S', f'/D={destination}'], logs / 'installer.log', timeout=300)
    executable = destination / 'auto-manager.exe'
    if not executable.is_file():
        raise RuntimeError('NSIS did not install the application at the requested location')
    with executable.open('rb') as binary:
        binary.seek(0x3C)
        pe_offset = struct.unpack('<I', binary.read(4))[0]
        binary.seek(pe_offset)
        if binary.read(4) != b'PE\0\0':
            raise RuntimeError('Installed Windows binary is not PE')
        machine = struct.unpack('<H', binary.read(2))[0]
    if machine != (0xAA64 if arch == 'aarch64' else 0x8664):
        raise RuntimeError('Installed Windows binary architecture mismatch')
    check_models(destination / 'ocr', 'NSIS')
    profile = work / 'profile'
    launch([executable], profile, logs)
    self_test([executable], profile, logs, 'NSIS')
    rehearse_update(harness, executable, installer, f'windows-{arch}-nsis', profile, logs)
    self_test([executable], profile, logs, 'NSIS after update')
    uninstaller = destination / 'uninstall.exe'
    if not uninstaller.is_file():
        raise RuntimeError('NSIS did not install its uninstaller')
    installed_files = [file for file in destination.rglob('*') if file.is_file()]
    run([uninstaller, '/S'], logs / 'uninstall.log', timeout=120)
    wait_uninstalled(installed_files)
    print('NSIS uninstall removed the application and its resources', flush=True)


def macos(bundle, arch, work, logs, harness):
    image = one(bundle, '*.dmg')
    mount = work / 'mount'
    mount.mkdir()
    attached = False
    try:
        run(['hdiutil', 'attach', image, '-readonly', '-nobrowse', '-mountpoint', mount], logs / 'installer.log')
        attached = True
        application = one(mount, '*.app')
        installed = work / application.name
        run(['ditto', application, installed], logs / 'installer.log')
    finally:
        if attached:
            run(['hdiutil', 'detach', mount], logs / 'installer.log')
    run(['codesign', '--verify', '--deep', '--strict', installed], logs / 'codesign.log')
    binary = installed / 'Contents' / 'MacOS' / 'auto-manager'
    description = run(['file', binary], logs / 'architecture.log')
    if ('arm64' if arch == 'aarch64' else 'x86_64') not in description:
        raise RuntimeError(f'Mach-O architecture mismatch: {description}')
    check_models(installed / 'Contents' / 'Resources' / 'ocr', 'DMG')
    profile = work / 'profile'
    launch([binary], profile, logs)
    self_test([binary], profile, logs, 'DMG')
    rehearse_update(harness, binary, one(bundle, '*.app.tar.gz'), f'darwin-{arch}-app', profile, logs, installed_app=installed)
    self_test([binary], profile, logs, 'DMG after update')


def in_container(image, package, script, logs, label):
    """Install a package on a clean distribution, run its self-test, and remove it."""
    run(['docker', 'run', '--rm', '-v', f'{package.parent}:/pkg:ro', '-e', f'PKG=/pkg/{package.name}',
         '-e', 'DEBIAN_FRONTEND=noninteractive', image, 'bash', '-ec', script], logs / f'{label}.log', timeout=900)
    print(f'{label}: installed on clean {image}, self-test passed, removed', flush=True)


# Run with `bash -ec`: every line must succeed.
SELF_TEST = '''
out=$(AUTO_MANAGER_DATA_DIR=$(mktemp -d) auto-manager --self-test)
echo "$out"
[[ "$out" == *'"integrity":"ok"'* ]]
test -f "/usr/share/applications/Auto Manager.desktop"
'''
GONE = 'if command -v auto-manager; then echo "still installed"; exit 1; fi\n'


def linux(bundle, arch, work, logs, harness):
    deb = one(bundle, '*.deb')
    rpm = one(bundle, '*.rpm')
    if run(['dpkg-deb', '-f', deb, 'Architecture'], logs / 'metadata.log').strip() != ('arm64' if arch == 'aarch64' else 'amd64'):
        raise RuntimeError('DEB architecture mismatch')
    if run(['rpm', '-qp', '--qf', '%{ARCH}', rpm], logs / 'metadata.log').strip() != arch:
        raise RuntimeError('RPM architecture mismatch')
    for listing in (run(['dpkg-deb', '-c', deb], logs / 'metadata.log'), run(['rpm', '-qlp', rpm], logs / 'metadata.log')):
        for name in OCR_MODELS:
            if f'Auto Manager/ocr/{name}' not in listing:
                raise RuntimeError(f'Linux package is missing the OCR model {name}')
    in_container('ubuntu:24.04', deb, 'apt-get update -qq\napt-get install -y -qq "$PKG" >/dev/null\n' + SELF_TEST
                 + 'apt-get remove -y -qq auto-manager >/dev/null\n' + GONE, logs, 'deb')
    in_container('fedora:latest', rpm, 'dnf install -y -q "$PKG"\n' + SELF_TEST + 'dnf remove -y -q auto-manager\n' + GONE,
                 logs, 'rpm')
    image = one(bundle, '*.AppImage')
    image.chmod(0o755)
    run([image, '--appimage-extract'], logs / 'extract.log', cwd=work)
    root = work / 'squashfs-root'
    check_models(root / 'usr' / 'lib' / 'Auto Manager' / 'ocr', 'AppImage')
    profile = work / 'profile'
    launch(['xvfb-run', '-a', '-s', '-screen 0 1280x800x24', 'dbus-run-session', '--', root / 'AppRun'],
           profile, logs, extra_env={'APPIMAGE': str(image)})
    self_test([root / 'AppRun'], profile, logs, 'AppImage')
    old_image = work / 'installed-old.AppImage'
    shutil.copy2(image, old_image)
    rehearse_update(harness, old_image, image, f'linux-{arch}-appimage', profile, logs)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--arch', choices=['x86_64', 'aarch64'], required=True)
    parser.add_argument('--logs', type=Path, required=True)
    parser.add_argument('--harness', type=Path, required=True)
    args = parser.parse_args()
    args.logs.mkdir(parents=True, exist_ok=True)
    args.logs = args.logs.resolve()
    if not args.harness.is_file():
        raise RuntimeError(f'Updater rehearsal harness missing: {args.harness}')
    with tempfile.TemporaryDirectory(prefix='am-smoke-', dir='/tmp' if sys.platform == 'darwin' else None) as temporary:
        work = Path(temporary).resolve()
        handler = windows if sys.platform == 'win32' else macos if sys.platform == 'darwin' else linux
        handler(args.bundle.resolve(), args.arch, work, args.logs, args.harness.resolve())
    print('Package smoke check passed', flush=True)


if __name__ == '__main__':
    main()

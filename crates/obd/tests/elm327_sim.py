#!/usr/bin/env python3
"""A stand-in ELM327 adapter on a pseudo-terminal, for testing the OBD client's real serial path.

Python standard library only. Prints the device path, then serves until stdin closes. Every
command received is printed as "CMD <command>" so the test can check what was sent. Behaves like
a v1.5 clone on a CAN car: echo on until ATE0, a slow ATZ banner, "SEARCHING..." on the first
request, multi-frame ISO-TP replies, NO DATA for unsupported PIDs, and replies dribbled out in
small chunks the way a real UART delivers them.
"""
import os, select, sys, threading, time, tty

VIN = "1HGCM82633A004352"
PIDS = {
    "0100": "41 00 BE 3F A8 13",
    "0101": "41 01 82 07 65 04",          # MIL on, 2 stored codes
    "0104": "41 04 3F",                   # load 24.7 %
    "0105": "41 05 7B",                   # coolant 83 °C
    "010B": "41 0B 21",                   # MAP 33 kPa
    "010C": "41 0C 1A F8",                # 1726 rpm
    "010D": "41 0D 00",                   # 0 km/h
    "010E": "41 0E 8C",                   # timing 6 °
    "010F": "41 0F 3D",                   # IAT 21 °C
    "0111": "41 11 26",                   # throttle 14.9 %
    "0142": "41 42 37 DC",                # 14.3 V
    "03": "43 02 01 33 03 00",            # P0133, P0300 (CAN: count byte first)
    "07": "47 00",
    "0902": "014\r0: 49 02 01 31 48 47\r1: 43 4D 38 32 36 33 33\r2: 41 30 30 34 33 35 32",
    "020200": "42 02 00 01 33",           # freeze frame stored by P0133
    "020C00": "42 0C 00 0B B8",           # 750 rpm at the time
    "020500": "42 05 00 5A",              # coolant 50 °C at the time
}

def main():
    master, slave = os.openpty()
    tty.setraw(slave)
    print(os.ttyname(slave), flush=True)
    state = {"echo": True, "searched": False}

    def send(text):
        data = text.encode()
        for i in range(0, len(data), 7):          # like a UART: a few bytes at a time
            os.write(master, data[i:i + 7])
            time.sleep(0.002)

    def answer(cmd):
        c = cmd.upper().replace(" ", "")
        if c == "ATZ":
            time.sleep(0.4)
            state["echo"] = True
            return "\r\rELM327 v1.5"
        if c == "ATE0":
            state["echo"] = False
            return "OK"
        if c in ("ATL0", "ATS1", "ATH0", "ATSP0", "ATAT1", "ATE1"):
            return "OK"
        if c == "ATDPN":
            return "A6"
        if c == "ATI":
            return "ELM327 v1.5"
        if c == "ATRV":
            return "14.3V"
        if c.startswith("AT"):
            return "?"
        reply = PIDS.get(c, "NO DATA")
        if not state["searched"]:
            state["searched"] = True
            time.sleep(0.3)
            reply = "SEARCHING...\r" + reply
        return reply

    def serve():
        buf = b""
        while True:
            r, _, _ = select.select([master], [], [], 0.5)
            if not r:
                continue
            try:
                chunk = os.read(master, 256)
            except OSError:
                return
            buf += chunk
            while b"\r" in buf:
                line, buf = buf.split(b"\r", 1)
                cmd = line.decode(errors="replace").strip()
                if not cmd:
                    continue
                print("CMD " + cmd, flush=True)
                out = (cmd + "\r" if state["echo"] else "") + answer(cmd) + "\r\r>"
                send(out)

    threading.Thread(target=serve, daemon=True).start()
    sys.stdin.read()          # until the test closes our stdin
    os.close(slave)

if __name__ == "__main__":
    main()

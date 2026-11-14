import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Camera, ImageUp } from "lucide-react";
import { call, errorMessage } from "../../api/ipc";
import type { ScanResult, VinFound } from "../../api/types";
import { Modal, Spinner } from "../../ui";
import { t } from "../../i18n";

type Want = "vin" | "barcode";

/** Read a VIN (or a part barcode) with the camera or from a photo. */
export function ScanDialog({ open, want, onClose, onVin, onBarcode }: { open: boolean; want: Want; onClose: () => void; onVin?: (vin: string) => void; onBarcode?: (code: string) => void }) {
  const video = useRef<HTMLVideoElement>(null);
  const [stream, setStream] = useState<MediaStream | null>(null);
  const [camErr, setCamErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ScanResult | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let s: MediaStream | null = null;
    let cancelled = false;
    setResult(null);
    setErr(null);
    setCamErr(null);
    if (!navigator.mediaDevices?.getUserMedia) {
      setCamErr(t("This computer's web view doesn't offer a camera. Choose a photo instead."));
      return;
    }
    navigator.mediaDevices
      .getUserMedia({ video: { facingMode: "environment", width: { ideal: 1920 }, height: { ideal: 1080 } }, audio: false })
      .then((m) => {
        if (cancelled) return m.getTracks().forEach((t) => t.stop());
        s = m;
        setStream(m);
      })
      .catch((e: Error) =>
        setCamErr(e.name === "NotAllowedError" ? t("Camera access was refused. Allow it in your system settings, or choose a photo instead.") : t("No camera is available. Choose a photo instead.")),
      );
    return () => {
      cancelled = true;
      s?.getTracks().forEach((t) => t.stop());
      setStream(null);
    };
  }, [open]);

  useEffect(() => {
    if (video.current && stream) {
      video.current.srcObject = stream;
      void video.current.play().catch(() => undefined);
    }
  }, [stream]);

  const read = async (f: () => Promise<ScanResult>) => {
    setBusy(true);
    setErr(null);
    try {
      setResult(await f());
    } catch (e) {
      setErr(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const capture = () =>
    read(async () => {
      const v = video.current;
      if (!v || !v.videoWidth) throw new Error(t("The camera picture isn't ready yet."));
      const c = document.createElement("canvas");
      c.width = v.videoWidth;
      c.height = v.videoHeight;
      c.getContext("2d")?.drawImage(v, 0, 0);
      const blob = await new Promise<Blob | null>((r) => c.toBlob(r, "image/jpeg", 0.92));
      if (!blob) throw new Error(t("Couldn't take a picture from the camera."));
      return invoke<ScanResult>("scan_frame", new Uint8Array(await blob.arrayBuffer()));
    });

  const photo = async () => {
    const path = await openDialog({ filters: [{ name: "Photos", extensions: ["jpg", "jpeg", "png", "webp"] }] });
    if (typeof path === "string") await read(() => call<ScanResult>("scan_image", { path }));
  };

  const useVin = (v: VinFound) => {
    onVin?.(v.vin);
    onClose();
  };
  const useCode = (code: string) => {
    onBarcode?.(code);
    onClose();
  };
  const nothing = result && (want === "vin" ? result.vins.length === 0 : result.barcodes.length === 0);

  return (
    <Modal open={open} onClose={onClose} title={want === "vin" ? t("Scan a VIN") : t("Scan a barcode")} wide>
      <div className="stack">
        <p className="small muted" style={{ margin: 0 }}>
          {want === "vin"
            ? t("Point the camera at the VIN plate (base of the windshield) or the barcode on the driver's door-jamb label, or choose a photo of either. Hold it steady and fill the frame.")
            : t("Point the camera at the part's barcode or QR code, or choose a photo of it.")}
        </p>
        {stream ? (
          <video ref={video} muted playsInline aria-label={t("Camera picture")} style={{ width: "100%", maxHeight: 360, background: "#000", borderRadius: 8 }} />
        ) : camErr ? (
          <div className="notice info">{camErr}</div>
        ) : (
          <Spinner label={t("Starting the camera")} />
        )}
        <div className="row">
          {stream && (
            <button className="btn primary" disabled={busy} onClick={capture}>
              <Camera size={16} aria-hidden /> {busy ? t("Reading…") : t("Read")}
            </button>
          )}
          <button className="btn" disabled={busy} onClick={photo}>
            <ImageUp size={16} aria-hidden />{" "}{t("Choose a photo…")}</button>
          {busy && <Spinner label={t("Reading the picture")} />}
        </div>
        {err && <div className="notice bad">{err}</div>}
        {result && want === "vin" && result.vins.length > 0 && (
          <div className="stack" style={{ gap: 6 }}>
            {result.vins.map((v) => (
              <div key={v.vin} className="spread card" style={{ padding: 10 }}>
                <span>
                  <strong className="mono" style={{ letterSpacing: "0.08em" }}>
                    {v.vin}
                  </strong>{" "}
                  <span className="small muted">
                    {v.from === "barcode" ? t("from the barcode") : t("read from the text")}
                    {v.corrected ? t(" · {0}: check it against the vehicle", v.corrected) : t(" · check digit correct")}
                  </span>
                </span>
                <button className="btn primary" onClick={() => useVin(v)}>{t("Use this VIN")}</button>
              </div>
            ))}
          </div>
        )}
        {result && want === "barcode" && result.barcodes.length > 0 && (
          <div className="stack" style={{ gap: 6 }}>
            {result.barcodes.map((b) => (
              <div key={b} className="spread card" style={{ padding: 10 }}>
                <strong className="mono">{b}</strong>
                <button className="btn primary" onClick={() => useCode(b)}>{t("Use this code")}</button>
              </div>
            ))}
          </div>
        )}
        {nothing && (
          <div className="notice warn">
            {want === "vin" ? t("No VIN with a correct check digit was found in this picture.") : t("No barcode was found in this picture.")}{" "}{t("Try again closer, straighter and with more light.")}{want === "vin" && !result.ocr && t(" (Reading printed text isn't available in this installation; door-jamb barcodes still work.)")}
            {want === "vin" && result.text.length > 0 && (
              <div className="small mt-1">{t("Text read:")}{" "}<span className="mono">{result.text.slice(0, 6).join(" · ")}</span>
              </div>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
}

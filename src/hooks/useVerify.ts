import { useState, useCallback } from "react";
import { verifyDevice, cancelVerify } from "../api/commands";
import { notifyDone } from "../utils/notify";

export interface VerifySummary {
  verified: number;
  baselined: number;
  modified: number;
  corrupted: number;
  missing: number;
}

export interface CorruptedFile {
  locationId: number;
  filePath: string;
  fileName: string;
}

export type VerifyPhase = "idle" | "running" | "done";

export function useVerify() {
  const [phase, setPhase] = useState<VerifyPhase>("idle");
  const [deviceId, setDeviceId] = useState("");
  const [progress, setProgress] = useState({ processed: 0, total: 0, currentFile: "" });
  const [summary, setSummary] = useState<VerifySummary | null>(null);
  const [corrupted, setCorrupted] = useState<CorruptedFile[]>([]);
  const [errors, setErrors] = useState<string[]>([]);

  const verify = useCallback(async (id: string, deviceLabel: string) => {
    setPhase("running");
    setDeviceId(id);
    setProgress({ processed: 0, total: 0, currentFile: "" });
    setSummary(null);
    setCorrupted([]);
    setErrors([]);
    try {
      let cancelled = false;
      await verifyDevice(id, (event) => {
        if (event === "Cancelled") {
          cancelled = true;
          return;
        }
        if ("Started" in event) {
          setProgress({ processed: 0, total: event.Started.total, currentFile: "" });
        } else if ("Progress" in event) {
          setProgress({
            processed: event.Progress.processed,
            total: event.Progress.total,
            currentFile: event.Progress.currentFile,
          });
        } else if ("Corrupted" in event) {
          setCorrupted((prev) => [...prev, event.Corrupted]);
        } else if ("Finished" in event) {
          setSummary(event.Finished);
          notifyDone(
            "Verification complete",
            event.Finished.corrupted > 0
              ? `${deviceLabel}: ${event.Finished.corrupted} possibly corrupted file${event.Finished.corrupted !== 1 ? "s" : ""}!`
              : `${deviceLabel}: all ${event.Finished.verified + event.Finished.baselined} files OK`
          );
          setPhase("done");
        } else if ("Error" in event) {
          setErrors((prev) => [...prev, event.Error.message]);
        }
      });
      if (cancelled) setPhase("idle");
    } catch (e) {
      setErrors((prev) => [...prev, String(e)]);
      setPhase("idle");
    }
  }, []);

  const cancel = useCallback(async () => {
    await cancelVerify();
  }, []);

  const reset = useCallback(() => {
    setPhase("idle");
    setSummary(null);
    setCorrupted([]);
    setErrors([]);
  }, []);

  return { phase, deviceId, progress, summary, corrupted, errors, verify, cancel, reset };
}

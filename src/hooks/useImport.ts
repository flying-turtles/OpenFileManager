import { useState, useCallback } from "react";
import type { ImportAnalysis, ImportEvent, DeviceCopyProgress } from "../types";
import {
  analyzeSdCard,
  startImport,
  cancelImport,
  ejectDevice,
} from "../api/commands";

export type ImportPhase =
  | "idle"
  | "analyzing"
  | "reviewed"
  | "copying"
  | "complete";

export function useImport() {
  const [phase, setPhase] = useState<ImportPhase>("idle");
  const [analysis, setAnalysis] = useState<ImportAnalysis | null>(null);
  const [analysisProgress, setAnalysisProgress] = useState({
    processed: 0,
    total: 0,
  });
  const [copyProgress, setCopyProgress] = useState<
    Record<string, DeviceCopyProgress>
  >({});
  const [errors, setErrors] = useState<string[]>([]);

  const handleEvent = useCallback((event: ImportEvent) => {
    if (typeof event === "string") {
      if (event === "CopyComplete") setPhase("complete");
      if (event === "Cancelled") setPhase("idle");
      return;
    }
    if ("AnalysisStarted" in event) {
      setAnalysisProgress({
        processed: 0,
        total: event.AnalysisStarted.total_files,
      });
    } else if ("AnalysisProgress" in event) {
      setAnalysisProgress({
        processed: event.AnalysisProgress.processed,
        total: event.AnalysisProgress.total,
      });
    } else if ("AnalysisComplete" in event) {
      setAnalysis(event.AnalysisComplete);
      setPhase("reviewed");
    } else if ("CopyProgress" in event) {
      setCopyProgress((prev) => ({
        ...prev,
        [event.CopyProgress.device_id]: event.CopyProgress,
      }));
    } else if ("Error" in event) {
      setErrors((prev) => [...prev, event.Error.message]);
    }
  }, []);

  const analyze = useCallback(
    async (sdMount: string) => {
      setPhase("analyzing");
      setErrors([]);
      setAnalysis(null);
      await analyzeSdCard(sdMount, handleEvent);
    },
    [handleEvent]
  );

  const startCopy = useCallback(
    async (deviceIds: string[]) => {
      setPhase("copying");
      setCopyProgress({});
      setErrors([]);
      await startImport(deviceIds, handleEvent);
    },
    [handleEvent]
  );

  const cancel = useCallback(async () => {
    await cancelImport();
  }, []);

  const eject = useCallback(async (mountPoint: string) => {
    await ejectDevice(mountPoint);
  }, []);

  const reset = useCallback(() => {
    setPhase("idle");
    setAnalysis(null);
    setAnalysisProgress({ processed: 0, total: 0 });
    setCopyProgress({});
    setErrors([]);
  }, []);

  return {
    phase,
    analysis,
    analysisProgress,
    copyProgress,
    errors,
    analyze,
    startCopy,
    cancel,
    eject,
    reset,
  };
}

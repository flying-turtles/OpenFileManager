import { useState, useCallback } from "react";
import type { MovePlan, MoveEvent, MoveResult, MovePhase } from "../types";
import { planMove, startMove, cancelMove } from "../api/commands";
import { notifyDone } from "../utils/notify";

export type MoveUiPhase = "idle" | "planning" | "planned" | "moving" | "complete" | "cancelled";

export interface MoveProgress {
  processed: number;
  total: number;
  bytesMoved: number;
  totalBytes: number;
  currentFile: string;
  phase: MovePhase;
}

export function useMove() {
  const [phase, setPhase] = useState<MoveUiPhase>("idle");
  const [plan, setPlan] = useState<MovePlan | null>(null);
  const [progress, setProgress] = useState<MoveProgress | null>(null);
  const [result, setResult] = useState<MoveResult | null>(null);
  const [errors, setErrors] = useState<string[]>([]);

  const handleEvent = useCallback((event: MoveEvent) => {
    if ("Cancelled" in event) {
      // Keep the partial tally: the page shows what the cancelled run moved.
      setResult(event.Cancelled);
      setPhase("cancelled");
      return;
    }
    if ("Progress" in event) {
      setProgress(event.Progress);
    } else if ("FileFailed" in event) {
      const f = event.FileFailed;
      setErrors((prev) => [...prev, `${f.fileName || "move"}: ${f.error}`]);
    } else if ("Complete" in event) {
      setResult(event.Complete);
      setPhase("complete");
      notifyDone(
        "Move complete",
        `${event.Complete.moved} file(s) moved and verified`
      );
    }
  }, []);

  const createPlan = useCallback(async (sources: string[], dest: string) => {
    setPhase("planning");
    setErrors([]);
    setResult(null);
    setProgress(null);
    try {
      const p = await planMove(sources, dest);
      setPlan(p);
      setPhase("planned");
    } catch (e: any) {
      setErrors([String(e)]);
      setPlan(null);
      setPhase("idle");
    }
  }, []);

  const start = useCallback(
    async (permanent: boolean) => {
      setPhase("moving");
      setErrors([]);
      setProgress(null);
      setResult(null);
      try {
        await startMove(permanent, handleEvent);
      } catch (e: any) {
        setErrors(prev => [...prev, String(e)]);
        setPhase("planned");
      }
    },
    [handleEvent]
  );

  const cancel = useCallback(async () => {
    await cancelMove();
  }, []);

  const reset = useCallback(() => {
    setPhase("idle");
    setPlan(null);
    setProgress(null);
    setResult(null);
    setErrors([]);
  }, []);

  return { phase, plan, progress, result, errors, createPlan, start, cancel, reset };
}

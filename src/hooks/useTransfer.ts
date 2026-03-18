import { useState, useCallback } from "react";
import type { TransferCheck, TransferEvent, DeviceCopyProgress } from "../types";
import {
  checkProjectTransfer,
  startProjectTransfer,
  cancelProjectTransfer,
} from "../api/commands";

export type TransferPhase =
  | "idle"
  | "select-device"
  | "unavailable"
  | "copying"
  | "complete"
  | "error";

export function useTransfer() {
  const [phase, setPhase] = useState<TransferPhase>("idle");
  const [projectId, setProjectId] = useState<number | null>(null);
  const [projectTitle, setProjectTitle] = useState("");
  const [selectedDeviceId, setSelectedDeviceId] = useState("");
  const [check, setCheck] = useState<TransferCheck | null>(null);
  const [progress, setProgress] = useState<DeviceCopyProgress | null>(null);
  const [errors, setErrors] = useState<string[]>([]);

  const handleEvent = useCallback((event: TransferEvent) => {
    if (event === "CopyComplete") {
      setPhase("complete");
    } else if (event === "Cancelled") {
      setPhase("select-device");
    } else if (typeof event === "object" && "CopyStarted" in event) {
      setPhase("copying");
    } else if (typeof event === "object" && "CopyProgress" in event) {
      setProgress(event.CopyProgress);
    } else if (typeof event === "object" && "Error" in event) {
      setErrors((prev) => [...prev, event.Error.message]);
    }
  }, []);

  const setProject = useCallback((id: number, title: string) => {
    setProjectId(id);
    setProjectTitle(title);
    setCheck(null);
    setProgress(null);
    setErrors([]);
    setSelectedDeviceId("");
    setPhase("select-device");
  }, []);

  const startCheck = useCallback(
    async (deviceId: string) => {
      if (!projectId) return;
      setSelectedDeviceId(deviceId);
      setErrors([]);
      setProgress(null);
      try {
        const c = await checkProjectTransfer(projectId, deviceId);
        setCheck(c);
        if (c.unavailableFiles.length > 0) {
          setPhase("unavailable");
        } else if (c.availableCount > 0) {
          setPhase("copying");
          await startProjectTransfer(handleEvent);
        } else {
          setPhase("complete");
        }
      } catch (e: any) {
        setErrors([e.toString()]);
        setPhase("error");
      }
    },
    [projectId, handleEvent]
  );

  const continueWithAvailable = useCallback(async () => {
    setErrors([]);
    setPhase("copying");
    try {
      await startProjectTransfer(handleEvent);
    } catch (e: any) {
      setErrors([e.toString()]);
      setPhase("error");
    }
  }, [handleEvent]);

  const cancel = useCallback(async () => {
    await cancelProjectTransfer();
  }, []);

  const reset = useCallback(() => {
    setPhase("idle");
    setProjectId(null);
    setProjectTitle("");
    setSelectedDeviceId("");
    setCheck(null);
    setProgress(null);
    setErrors([]);
  }, []);

  return {
    phase,
    projectId,
    projectTitle,
    selectedDeviceId,
    check,
    progress,
    errors,
    setProject,
    startCheck,
    continueWithAvailable,
    cancel,
    reset,
  };
}

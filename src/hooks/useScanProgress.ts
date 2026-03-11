import { useState, useCallback, useRef } from "react";
import type { ScanEvent } from "../types";
import { startScan, cancelScan, pauseScan } from "../api/commands";

export interface ScanState {
  scanning: boolean;
  paused: boolean;
  total: number;
  scanned: number;
  hashed: number;
  added: number;
  removed: number;
  lastFile: string;
  error: string | null;
  finished: boolean;
}

const initialState: ScanState = {
  scanning: false,
  paused: false,
  total: 0,
  scanned: 0,
  hashed: 0,
  added: 0,
  removed: 0,
  lastFile: "",
  error: null,
  finished: false,
};

export function useScanProgress() {
  const [state, setState] = useState<ScanState>(initialState);
  const scanningRef = useRef(false);

  const scan = useCallback(async (target: string, mode: string) => {
    setState({ ...initialState, scanning: true });
    scanningRef.current = true;

    const onEvent = (event: ScanEvent) => {
      if (event === "Cancelled") {
        setState((s) => ({ ...s, scanning: false, error: "Scan cancelled" }));
        scanningRef.current = false;
        return;
      }
      if (typeof event === "object" && "Paused" in event) {
        setState((s) => ({
          ...s,
          scanning: false,
          paused: true,
          scanned: event.Paused.scanned,
          hashed: event.Paused.hashed,
          added: event.Paused.added,
          total: event.Paused.total,
        }));
        scanningRef.current = false;
        return;
      }
      if (typeof event === "object" && "Started" in event) {
        setState((s) => ({ ...s, total: event.Started.total_files }));
      } else if (typeof event === "object" && "Progress" in event) {
        setState((s) => ({
          ...s,
          scanned: event.Progress.scanned,
          total: event.Progress.total,
        }));
      } else if (typeof event === "object" && "FileHashed" in event) {
        setState((s) => ({
          ...s,
          hashed: s.hashed + 1,
          lastFile: event.FileHashed.path,
        }));
      } else if (typeof event === "object" && "Finished" in event) {
        setState((s) => ({
          ...s,
          scanning: false,
          finished: true,
          scanned: event.Finished.scanned,
          hashed: event.Finished.hashed,
          added: event.Finished.added,
          removed: event.Finished.removed,
        }));
        scanningRef.current = false;
      } else if (typeof event === "object" && "Error" in event) {
        setState((s) => ({ ...s, error: event.Error.message }));
      }
    };

    try {
      await startScan(target, mode, onEvent);
    } catch (e) {
      setState((s) => ({
        ...s,
        scanning: false,
        error: String(e),
      }));
      scanningRef.current = false;
    }
  }, []);

  const cancel = useCallback(async () => {
    if (scanningRef.current) {
      await cancelScan();
    }
  }, []);

  const pause = useCallback(async () => {
    if (scanningRef.current) {
      await pauseScan();
    }
  }, []);

  const reset = useCallback(() => {
    setState(initialState);
  }, []);

  return { ...state, scan, cancel, pause, reset };
}

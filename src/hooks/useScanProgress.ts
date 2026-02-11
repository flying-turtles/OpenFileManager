import { useState, useCallback, useRef } from "react";
import type { ScanEvent } from "../types";
import { startScan, cancelScan } from "../api/commands";

export interface ScanState {
  scanning: boolean;
  total: number;
  scanned: number;
  hashed: number;
  lastFile: string;
  error: string | null;
  finished: boolean;
}

const initialState: ScanState = {
  scanning: false,
  total: 0,
  scanned: 0,
  hashed: 0,
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
      if ("Started" in event) {
        setState((s) => ({ ...s, total: event.Started.total_files }));
      } else if ("Progress" in event) {
        setState((s) => ({
          ...s,
          scanned: event.Progress.scanned,
          total: event.Progress.total,
        }));
      } else if ("FileHashed" in event) {
        setState((s) => ({
          ...s,
          hashed: s.hashed + 1,
          lastFile: event.FileHashed.path,
        }));
      } else if ("Finished" in event) {
        setState((s) => ({
          ...s,
          scanning: false,
          finished: true,
          scanned: event.Finished.scanned,
          hashed: event.Finished.hashed,
        }));
        scanningRef.current = false;
      } else if ("Error" in event) {
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

  const reset = useCallback(() => {
    setState(initialState);
  }, []);

  return { ...state, scan, cancel, reset };
}

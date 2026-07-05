import { useState, useCallback } from "react";
import type { SimilarGroup, BulkDeleteEvent, BulkDeleteResult } from "../types";
import {
  scanSimilarPictures,
  cancelSimilarScan,
  getSimilarGroups,
  bulkDeleteFileCopies,
} from "../api/commands";
import { notifyDone } from "../utils/notify";

export type SimilarPhase = "idle" | "scanning" | "loading" | "ready";

export function useSimilar() {
  const [phase, setPhase] = useState<SimilarPhase>("idle");
  const [progress, setProgress] = useState({ processed: 0, total: 0 });
  const [groups, setGroups] = useState<SimilarGroup[]>([]);
  const [error, setError] = useState("");

  const loadGroups = useCallback(async (maxDistance: number, deviceId?: string) => {
    setPhase("loading");
    setError("");
    try {
      setGroups(await getSimilarGroups(maxDistance, deviceId));
      setPhase("ready");
    } catch (e) {
      setError(String(e));
      setPhase("idle");
    }
  }, []);

  const scan = useCallback(
    async (maxDistance: number, deviceId?: string) => {
      setPhase("scanning");
      setError("");
      setProgress({ processed: 0, total: 0 });
      try {
        let cancelled = false;
        await scanSimilarPictures((event) => {
          if (event === "Cancelled") {
            cancelled = true;
            return;
          }
          if ("Started" in event) {
            setProgress({ processed: 0, total: event.Started.total });
          } else if ("Progress" in event) {
            setProgress({
              processed: event.Progress.processed,
              total: event.Progress.total,
            });
          } else if ("Finished" in event) {
            notifyDone(
              "Similarity scan complete",
              `${event.Finished.hashed} items analyzed`
            );
          } else if ("Error" in event) {
            setError(event.Error.message);
          }
        }, deviceId);
        if (cancelled) {
          setPhase("idle");
          return;
        }
        await loadGroups(maxDistance, deviceId);
      } catch (e) {
        setError(String(e));
        setPhase("idle");
      }
    },
    [loadGroups]
  );

  const cancel = useCallback(async () => {
    await cancelSimilarScan();
  }, []);

  const deleteFiles = useCallback(
    async (
      locationIds: number[],
      onEvent: (event: BulkDeleteEvent) => void
    ): Promise<BulkDeleteResult> => {
      const result = await bulkDeleteFileCopies(locationIds, onEvent);
      const succeeded = new Set(result.succeeded);
      // Drop deleted locations; drop files with none left; drop groups with < 2 files
      setGroups((prev) =>
        prev
          .map((g) => ({
            files: g.files
              .map((f) => ({
                ...f,
                locations: f.locations.filter((l) => !succeeded.has(l.id)),
              }))
              .filter((f) => f.locations.length > 0),
          }))
          .filter((g) => g.files.length > 1)
      );
      return result;
    },
    []
  );

  return { phase, progress, groups, error, scan, cancel, loadGroups, deleteFiles };
}

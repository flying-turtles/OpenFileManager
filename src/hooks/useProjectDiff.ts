import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import {
  computeProjectDiff,
  cancelProjectDiff,
  copyDiffFiles,
  purgeDiffLocations,
  bulkDeleteFileCopies,
} from "../api/commands";
import { notifyDone } from "../utils/notify";
import type {
  ProjectDiff,
  DiffFileEntry,
  DiffEvent,
  DiffCopyItem,
  DiffCopyResult,
  BulkDeleteResult,
} from "../types";

/** Identifies one entry across the whole diff. */
export function entryKey(deviceId: string, relativePath: string): string {
  return `${deviceId}>${relativePath}`;
}

export interface TreeNode {
  /** Folder path relative to the device mount. Empty at the root. */
  path: string;
  name: string;
  children: TreeNode[];
  files: DiffFileEntry[];
}

/** Groups entries into a folder tree by their relative path. */
export function buildTree(entries: DiffFileEntry[]): TreeNode {
  const root: TreeNode = { path: "", name: "", children: [], files: [] };

  for (const entry of entries) {
    const parts = entry.relativePath.split("/");
    const dirs = parts.slice(0, -1);
    let node = root;
    let sofar = "";
    for (const dir of dirs) {
      sofar = sofar ? `${sofar}/${dir}` : dir;
      let child = node.children.find((c) => c.name === dir);
      if (!child) {
        child = { path: sofar, name: dir, children: [], files: [] };
        node.children.push(child);
      }
      node = child;
    }
    node.files.push(entry);
  }

  const sortNode = (node: TreeNode) => {
    node.children.sort((a, b) => a.name.localeCompare(b.name));
    node.files.sort((a, b) => a.fileName.localeCompare(b.fileName));
    node.children.forEach(sortNode);
  };
  sortNode(root);

  return root;
}

/** Every entry key at or below this node. */
export function collectKeys(deviceId: string, node: TreeNode): string[] {
  const keys = node.files.map((f) => entryKey(deviceId, f.relativePath));
  for (const child of node.children) {
    keys.push(...collectKeys(deviceId, child));
  }
  return keys;
}

/** Checkbox state for a folder, given the keys underneath it. */
export function folderState(
  keys: string[],
  selected: Set<string>
): "all" | "none" | "some" {
  if (keys.length === 0) return "none";
  let hits = 0;
  for (const k of keys) if (selected.has(k)) hits++;
  if (hits === 0) return "none";
  if (hits === keys.length) return "all";
  return "some";
}

export type DiffPhase = "idle" | "computing" | "ready" | "resolving" | "done" | "error";

export interface DiffProgress {
  checked: number;
  total: number;
  currentDevice: string;
}

export interface ResolveResult {
  deleted: BulkDeleteResult | null;
  copied: DiffCopyResult | null;
  purged: number;
}

export function useProjectDiff(projectId: number, sotDeviceId: string) {
  const [phase, setPhase] = useState<DiffPhase>("idle");
  const [diff, setDiff] = useState<ProjectDiff | null>(null);
  const [progress, setProgress] = useState<DiffProgress | null>(null);
  const [error, setError] = useState("");
  const [selectedDelete, setSelectedDelete] = useState<Set<string>>(new Set());
  const [selectedCopy, setSelectedCopy] = useState<Set<string>>(new Set());
  const [resolveProgress, setResolveProgress] = useState("");
  const [resolveResult, setResolveResult] = useState<ResolveResult | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  // Identifies the current effect run. A boolean can't distinguish "run A"
  // from "run B" — React fires cleanup(A) and then body(B) back-to-back in
  // the same tick, so a shared flag set true by A's cleanup is immediately
  // set false again by B's body, and a late event from A reads through.
  // A monotonically increasing id per run fixes that: each run captures its
  // own id and only fires state setters while that id is still current.
  const runIdRef = useRef(0);

  useEffect(() => {
    const runId = ++runIdRef.current;
    setPhase("computing");
    setError("");
    setDiff(null);
    setProgress(null);
    setResolveResult(null);

    // Defined inline so it closes over this run's `runId` — the simplest
    // way to give it a per-run identity check without adding a state
    // dependency (a dependency would re-wire the channel on every tick).
    const handleEvent = (event: DiffEvent) => {
      if (runId !== runIdRef.current) return;
      if (typeof event === "string") return;
      if ("Progress" in event) setProgress(event.Progress);
      else if ("Error" in event) setError(event.Error.message);
    };

    computeProjectDiff(projectId, sotDeviceId, handleEvent)
      .then((d) => {
        if (runId !== runIdRef.current) return;
        setDiff(d);
        // Both directions start fully selected: the point of the feature is
        // to bring the backups in line unless the user says otherwise.
        const del = new Set<string>();
        const cop = new Set<string>();
        for (const device of d.devices) {
          for (const e of device.toDelete) del.add(entryKey(device.deviceId, e.relativePath));
          for (const e of device.toCopy) cop.add(entryKey(device.deviceId, e.relativePath));
        }
        setSelectedDelete(del);
        setSelectedCopy(cop);
        setPhase("ready");
      })
      .catch((e) => {
        if (runId !== runIdRef.current) return;
        setError(String(e));
        setPhase("error");
      });

    // Deliberately no cancelProjectDiff() call here. cleanup(A) and the next
    // body(B) race the backend with no ordering guarantee, and the backend
    // keeps a single cancel-token slot that each new compute overwrites —
    // a cleanup-time cancel can end up cancelling B's fresh run instead of
    // A's abandoned one. So the abandoned run is left to keep probing until
    // it finishes on its own; that's fine, it's read-only and every probe
    // is deadline-guarded. Bumping runIdRef below is what actually matters:
    // it stops the abandoned run's events from reaching state. The Cancel
    // button (`cancel()`) still calls cancelProjectDiff() directly and
    // still works, since nothing else is starting a run at that moment.
    return () => {
      runIdRef.current += 1;
    };
  }, [projectId, sotDeviceId, reloadKey]);

  const toggleKeys = useCallback(
    (section: "delete" | "copy", keys: string[], next: boolean) => {
      const setter = section === "delete" ? setSelectedDelete : setSelectedCopy;
      setter((prev) => {
        const out = new Set(prev);
        for (const k of keys) {
          if (next) out.add(k);
          else out.delete(k);
        }
        return out;
      });
    },
    []
  );

  const setAll = useCallback(
    (section: "delete" | "copy", next: boolean) => {
      if (!diff) return;
      const keys: string[] = [];
      for (const device of diff.devices) {
        const entries = section === "delete" ? device.toDelete : device.toCopy;
        for (const e of entries) keys.push(entryKey(device.deviceId, e.relativePath));
      }
      toggleKeys(section, keys, next);
    },
    [diff, toggleKeys]
  );

  const deleteLocationIds = useMemo(() => {
    if (!diff) return [];
    const ids: number[] = [];
    for (const device of diff.devices) {
      for (const e of device.toDelete) {
        if (e.locationId !== null && selectedDelete.has(entryKey(device.deviceId, e.relativePath))) {
          ids.push(e.locationId);
        }
      }
    }
    return ids;
  }, [diff, selectedDelete]);

  const copyItems = useMemo<DiffCopyItem[]>(() => {
    if (!diff) return [];
    const items: DiffCopyItem[] = [];
    for (const device of diff.devices) {
      for (const e of device.toCopy) {
        if (e.sourcePath && selectedCopy.has(entryKey(device.deviceId, e.relativePath))) {
          items.push({
            blake3Hash: e.blake3Hash,
            fileSize: e.fileSize,
            fileName: e.fileName,
            sourcePath: e.sourcePath,
            targetDeviceId: device.deviceId,
            relativePath: e.relativePath,
          });
        }
      }
    }
    return items;
  }, [diff, selectedCopy]);

  const resolve = useCallback(
    async (permanent: boolean) => {
      if (!diff) return;
      setPhase("resolving");
      setError("");
      let deleted: BulkDeleteResult | null = null;
      let copied: DiffCopyResult | null = null;
      let purged = 0;
      const errors: string[] = [];

      try {
        try {
          if (deleteLocationIds.length > 0) {
            setResolveProgress("Deleting from backups...");
            deleted = await bulkDeleteFileCopies(
              deleteLocationIds,
              (event) => {
                if ("Progress" in event) {
                  setResolveProgress(
                    `Deleting ${event.Progress.processed} / ${event.Progress.total}`
                  );
                }
              },
              permanent
            );
          }
        } catch (e: any) {
          errors.push(String(e));
        }

        try {
          if (copyItems.length > 0) {
            setResolveProgress("Copying to backups...");
            copied = await copyDiffFiles(copyItems, (event) => {
              if (typeof event !== "string" && "Progress" in event) {
                const p = event.Progress;
                setResolveProgress(
                  `Copying ${p.filesCopied} / ${p.totalFiles} - ${p.currentFile}`
                );
              }
            });
          }
        } catch (e: any) {
          errors.push(String(e));
        }

        // Unconditional: an index row pointing at a file that's gone from
        // disk is wrong whether or not delete/copy above succeeded, so the
        // purge must run even after one of them throws.
        try {
          if (diff.purgeLocationIds.length > 0) {
            setResolveProgress("Updating the index...");
            purged = await purgeDiffLocations(diff.purgeLocationIds);
          }
        } catch (e: any) {
          errors.push(String(e));
        }

        setResolveResult({ deleted, copied, purged });
        setPhase("done");
        if (errors.length > 0) {
          setError(errors.join("; "));
        } else {
          notifyDone(
            "Diff resolved",
            `${deleted?.succeeded.length ?? 0} deleted, ${copied?.copied ?? 0} copied`
          );
        }
      } finally {
        setResolveProgress("");
      }
    },
    [diff, deleteLocationIds, copyItems]
  );

  const cancel = useCallback(() => {
    cancelProjectDiff().catch(() => {});
  }, []);

  const reload = useCallback(() => setReloadKey((k) => k + 1), []);

  return {
    phase,
    diff,
    progress,
    error,
    selectedDelete,
    selectedCopy,
    deleteLocationIds,
    copyItems,
    toggleKeys,
    setAll,
    resolve,
    resolveProgress,
    resolveResult,
    cancel,
    reload,
  };
}

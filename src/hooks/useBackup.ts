import { useState, useEffect, useCallback } from "react";
import type { BackupSettings } from "../types";
import {
  getBackupSettings,
  saveBackupSettings,
  testBackupConnection,
  runDatabaseBackup,
} from "../api/commands";

export type BackupPhase = "idle" | "running" | "done" | "error";

export interface BackupProgress {
  table: string;
  rowsCopied: number;
  totalRows: number;
  tablesDone: number;
  totalTables: number;
}

export function useBackup() {
  const [settings, setSettings] = useState<BackupSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [phase, setPhase] = useState<BackupPhase>("idle");
  const [progress, setProgress] = useState<BackupProgress | null>(null);
  const [error, setError] = useState("");
  const [totalRows, setTotalRows] = useState(0);

  useEffect(() => {
    getBackupSettings()
      .then(setSettings)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  const save = useCallback(
    async (host: string, port: number, database: string, username: string, password: string) => {
      const saved = await saveBackupSettings(host, port, database, username, password);
      setSettings(saved);
      return saved;
    },
    []
  );

  const test = useCallback(async () => {
    await testBackupConnection();
  }, []);

  const backup = useCallback(async () => {
    setPhase("running");
    setError("");
    setProgress(null);
    setTotalRows(0);
    let tablesDone = 0;
    let totalTables = 0;
    try {
      await runDatabaseBackup((event) => {
        if ("Started" in event) {
          totalTables = event.Started.totalTables;
          setProgress({ table: "", rowsCopied: 0, totalRows: 0, tablesDone, totalTables });
        } else if ("TableProgress" in event) {
          setProgress({
            table: event.TableProgress.table,
            rowsCopied: event.TableProgress.rowsCopied,
            totalRows: event.TableProgress.totalRows,
            tablesDone,
            totalTables,
          });
        } else if ("TableDone" in event) {
          tablesDone += 1;
          setProgress((p) => (p ? { ...p, tablesDone } : p));
        } else if ("Finished" in event) {
          setTotalRows(event.Finished.totalRows);
          setSettings((s) => (s ? { ...s, lastBackupAt: event.Finished.finishedAt } : s));
          setPhase("done");
        } else if ("Error" in event) {
          setError(event.Error.message);
          setPhase("error");
        }
      });
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }, []);

  const resetStatus = useCallback(() => {
    setPhase("idle");
    setError("");
    setProgress(null);
  }, []);

  return { settings, loading, phase, progress, error, totalRows, save, test, backup, resetStatus };
}

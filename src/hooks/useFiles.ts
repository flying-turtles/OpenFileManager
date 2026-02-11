import { useState, useCallback } from "react";
import type { FileLocation, FileSafety, WasteCandidate } from "../types";
import {
  getFilesOnDevice,
  getUnsafeFiles,
  getWasteCandidates,
  getFileSafety,
  getFileLocations,
} from "../api/commands";

export function useFiles() {
  const [files, setFiles] = useState<FileLocation[]>([]);
  const [unsafeFiles, setUnsafeFiles] = useState<FileSafety[]>([]);
  const [waste, setWaste] = useState<WasteCandidate[]>([]);
  const [loading, setLoading] = useState(false);

  const loadDeviceFiles = useCallback(async (deviceId: string) => {
    setLoading(true);
    try {
      const f = await getFilesOnDevice(deviceId);
      setFiles(f);
    } finally {
      setLoading(false);
    }
  }, []);

  const loadUnsafeFiles = useCallback(async () => {
    setLoading(true);
    try {
      const f = await getUnsafeFiles();
      setUnsafeFiles(f);
    } finally {
      setLoading(false);
    }
  }, []);

  const loadWaste = useCallback(async (threshold?: number) => {
    setLoading(true);
    try {
      const w = await getWasteCandidates(threshold);
      setWaste(w);
    } finally {
      setLoading(false);
    }
  }, []);

  const loadFileSafety = useCallback(async (hash: string) => {
    return getFileSafety(hash);
  }, []);

  const loadLocations = useCallback(async (hash: string) => {
    return getFileLocations(hash);
  }, []);

  return {
    files,
    unsafeFiles,
    waste,
    loading,
    loadDeviceFiles,
    loadUnsafeFiles,
    loadWaste,
    loadFileSafety,
    loadLocations,
  };
}

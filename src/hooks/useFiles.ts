import { useState, useCallback, useRef } from "react";
import type { FileLocation, FileSafety, WasteCandidate } from "../types";
import {
  getFilesOnDevicePage,
  getUnsafeFilesPage,
  getSafeFilesPage,
  getDuplicateFilesPage,
  deleteFileCopy as deleteFileCopyApi,
  getWasteCandidates,
  getFileSafety,
  getFileLocations,
} from "../api/commands";

export function useFiles() {
  const [files, setFiles] = useState<FileLocation[]>([]);
  const [unsafeFiles, setUnsafeFiles] = useState<FileSafety[]>([]);
  const [safeFiles, setSafeFiles] = useState<FileSafety[]>([]);
  const [duplicateFiles, setDuplicateFiles] = useState<FileSafety[]>([]);
  const [waste, setWaste] = useState<WasteCandidate[]>([]);
  const [loading, setLoading] = useState(false);
  const [totalCount, setTotalCount] = useState(0);
  const loadIdRef = useRef(0);

  const loadDeviceFiles = useCallback(async (deviceId: string) => {
    const loadId = ++loadIdRef.current;
    setLoading(true);
    setFiles([]);
    setTotalCount(0);
    try {
      let cursor: string | undefined;
      let accumulated: FileLocation[] = [];
      let firstPage = true;
      do {
        const page = await getFilesOnDevicePage(deviceId, cursor);
        if (loadId !== loadIdRef.current) return;
        accumulated = [...accumulated, ...page.files];
        setFiles(accumulated);
        setTotalCount(page.total);
        cursor = page.nextCursor ?? undefined;
        if (firstPage) {
          setLoading(false);
          firstPage = false;
        }
      } while (cursor);
    } finally {
      if (loadId === loadIdRef.current) setLoading(false);
    }
  }, []);

  const loadUnsafeFiles = useCallback(async () => {
    const loadId = ++loadIdRef.current;
    setLoading(true);
    setUnsafeFiles([]);
    setTotalCount(0);
    try {
      let offset = 0;
      let accumulated: FileSafety[] = [];
      let hasMore = true;
      let firstPage = true;
      while (hasMore) {
        const page = await getUnsafeFilesPage(offset);
        if (loadId !== loadIdRef.current) return;
        accumulated = [...accumulated, ...page.files];
        setUnsafeFiles(accumulated);
        setTotalCount(page.total);
        hasMore = page.hasMore;
        offset += page.files.length;
        if (firstPage) {
          setLoading(false);
          firstPage = false;
        }
      }
    } finally {
      if (loadId === loadIdRef.current) setLoading(false);
    }
  }, []);

  const loadSafeFiles = useCallback(async () => {
    const loadId = ++loadIdRef.current;
    setLoading(true);
    setSafeFiles([]);
    setTotalCount(0);
    try {
      let offset = 0;
      let accumulated: FileSafety[] = [];
      let hasMore = true;
      let firstPage = true;
      while (hasMore) {
        const page = await getSafeFilesPage(offset);
        if (loadId !== loadIdRef.current) return;
        accumulated = [...accumulated, ...page.files];
        setSafeFiles(accumulated);
        setTotalCount(page.total);
        hasMore = page.hasMore;
        offset += page.files.length;
        if (firstPage) {
          setLoading(false);
          firstPage = false;
        }
      }
    } finally {
      if (loadId === loadIdRef.current) setLoading(false);
    }
  }, []);

  const loadDuplicateFiles = useCallback(async (deviceId?: string) => {
    const loadId = ++loadIdRef.current;
    setLoading(true);
    setDuplicateFiles([]);
    setTotalCount(0);
    try {
      let offset = 0;
      let accumulated: FileSafety[] = [];
      let hasMore = true;
      let firstPage = true;
      while (hasMore) {
        const page = await getDuplicateFilesPage(offset, undefined, deviceId);
        if (loadId !== loadIdRef.current) return;
        accumulated = [...accumulated, ...page.files];
        setDuplicateFiles(accumulated);
        setTotalCount(page.total);
        hasMore = page.hasMore;
        offset += page.files.length;
        if (firstPage) {
          setLoading(false);
          firstPage = false;
        }
      }
    } finally {
      if (loadId === loadIdRef.current) setLoading(false);
    }
  }, []);

  const deleteFileCopy = useCallback(async (locationId: number) => {
    await deleteFileCopyApi(locationId);
    // Remove from local state
    setDuplicateFiles((prev) =>
      prev
        .map((sf) => ({
          ...sf,
          totalCopies: sf.totalCopies - (sf.locations.some((l) => l.id === locationId) ? 1 : 0),
          locations: sf.locations.filter((l) => l.id !== locationId),
        }))
        .filter((sf) => sf.locations.length > 0)
    );
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
    safeFiles,
    duplicateFiles,
    waste,
    loading,
    totalCount,
    loadDeviceFiles,
    loadUnsafeFiles,
    loadSafeFiles,
    loadDuplicateFiles,
    deleteFileCopy,
    loadWaste,
    loadFileSafety,
    loadLocations,
  };
}

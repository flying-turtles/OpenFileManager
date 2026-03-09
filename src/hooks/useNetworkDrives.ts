import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import type { NetworkDrive } from "../types";
import {
  getNetworkDrives,
  addNetworkDrive as apiAdd,
  mountNetworkDrive as apiMount,
  unmountNetworkDrive as apiUnmount,
  removeNetworkDrive as apiRemove,
} from "../api/commands";

export function useNetworkDrives() {
  const [drives, setDrives] = useState<NetworkDrive[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const d = await getNetworkDrives();
      setDrives(d);
    } catch (e) {
      console.error("Failed to load network drives:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  const add = useCallback(
    async (
      protocol: string,
      host: string,
      sharePath: string,
      username: string,
      password: string,
      label: string,
      deviceType: string
    ) => {
      const drive = await apiAdd(protocol, host, sharePath, username, password, label, deviceType);
      setDrives((prev) => [drive, ...prev]);
      return drive;
    },
    []
  );

  const mount = useCallback(async (id: string) => {
    await apiMount(id);
    setDrives((prev) => prev.map((d) => (d.id === id ? { ...d, is_mounted: true } : d)));
  }, []);

  const unmount = useCallback(async (id: string) => {
    await apiUnmount(id);
    setDrives((prev) => prev.map((d) => (d.id === id ? { ...d, is_mounted: false } : d)));
  }, []);

  const remove = useCallback(async (id: string) => {
    await apiRemove(id);
    setDrives((prev) => prev.filter((d) => d.id !== id));
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen("network-drive-status", () => refresh());
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh]);

  return { drives, loading, refresh, add, mount, unmount, remove };
}

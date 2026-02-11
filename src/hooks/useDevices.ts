import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import type { StorageDevice } from "../types";
import { detectDevices, getDevices, setDeviceType as apiSetDeviceType } from "../api/commands";

export function useDevices() {
  const [devices, setDevices] = useState<StorageDevice[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const devs = await detectDevices();
      setDevices(devs);
    } catch (e) {
      console.error("Failed to detect devices:", e);
      // Fall back to cached devices
      const devs = await getDevices();
      setDevices(devs);
    } finally {
      setLoading(false);
    }
  }, []);

  const setType = useCallback(
    async (deviceId: string, deviceType: string) => {
      await apiSetDeviceType(deviceId, deviceType);
      setDevices((prev) =>
        prev.map((d) => (d.id === deviceId ? { ...d, device_type: deviceType } : d))
      );
    },
    []
  );

  useEffect(() => {
    refresh();
    const unlisten = listen("devices-changed", () => refresh());
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh]);

  return { devices, loading, refresh, setType };
}

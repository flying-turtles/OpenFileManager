import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import type { StorageDevice } from "../types";
import { detectDevices, getDevices, setDeviceType as apiSetDeviceType, setDriveSpeed as apiSetDriveSpeed, removeDevice as apiRemoveDevice } from "../api/commands";

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
        prev.map((d) => (d.id === deviceId ? { ...d, deviceType: deviceType } : d))
      );
    },
    []
  );

  const setSpeed = useCallback(
    async (deviceId: string, driveSpeed: string) => {
      await apiSetDriveSpeed(deviceId, driveSpeed);
      setDevices((prev) =>
        prev.map((d) => (d.id === deviceId ? { ...d, driveSpeed } : d))
      );
    },
    []
  );

  const remove = useCallback(async (deviceId: string) => {
    await apiRemoveDevice(deviceId);
    setDevices((prev) => prev.filter((d) => d.id !== deviceId));
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen("devices-changed", () => refresh());
    const onFocus = () => refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      unlisten.then((fn) => fn());
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  return { devices, loading, refresh, setType, setSpeed, remove };
}

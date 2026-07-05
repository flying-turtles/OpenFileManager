import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

let permission: boolean | null = null;

/// Fire a native notification for a finished long-running operation.
/// Only notifies when the window is not focused — if the user is watching
/// the progress bar, a notification is just noise.
export async function notifyDone(title: string, body: string): Promise<void> {
  try {
    if (document.hasFocus()) return;
    if (permission === null) {
      permission = await isPermissionGranted();
      if (!permission) {
        permission = (await requestPermission()) === "granted";
      }
    }
    if (permission) {
      sendNotification({ title, body });
    }
  } catch (e) {
    console.error("Notification failed:", e);
  }
}

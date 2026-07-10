# Device Reconnect — Design

Date: 2026-07-10
Status: Approved (Approach A: two-command check-then-commit)

## Problem

A registered device (external disk, manually added location) can reappear under a
different path than the one stored in `storage_devices.mount_point` — renamed mount
(`Media-1` → `Media-2`), network share mounted elsewhere, moved folder. Auto-detection
only covers top-level `/Volumes/*` with a `.filemanagerid` marker. Everywhere else the
device shows disconnected forever, even though all `file_locations` rows (paths are
relative to `mount_point`) would be valid again if `mount_point` were updated.

Goal: manual "Reconnect" in Devices tab — pick new path, validate, update
`mount_point`. DB references stay valid; no rescan needed.

## Scope

- Reconnect button on `DeviceCard` only when device is disconnected.
- Network drives (`NetworkDriveCard`) excluded — they have their own mount logic.
- Validation: smart check + force. Marker match → accept silently-ish; marker
  mismatch/missing → warn with evidence (sampled-file check), user may force-connect
  (which writes/updates the id marker).

## Backend (Rust)

### New command: `check_reconnect_target(device_id, new_path) -> ReconnectCheck`

In `src-tauri/src/commands/devices.rs`.

1. Guard: `path_online(&new_path).await` — never sync `.exists()` in async commands
   (network mounts stall). Offline → `AppError` "Path is not reachable".
2. Marker: read `<new_path>/.filemanagerid` (exact directory, no walk-up — user picked
   the root explicitly). Compare to `device_id`:
   - equals → `marker_status = "match"`
   - different id → `"mismatch"` (include the foreign id in response)
   - no file → `"missing"`
3. Sample check: load up to 20 `file_locations` rows for `device_id` (existing
   `db::get_files_on_device` or a `LIMIT 20` variant), join each `file_path` onto
   `new_path`, check existence inside `tokio::task::spawn_blocking` (bounded work; path
   already proven online in step 1). Return counts.

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectCheck {
    marker_status: String,        // "match" | "mismatch" | "missing"
    foreign_id: Option<String>,   // set when mismatch
    found_files: i64,
    sampled_files: i64,
}
```

### New command: `reconnect_device(device_id, new_path) -> StorageDevice`

1. Guard `path_online`.
2. Conflict check: reject if `new_path` equals another device's `mount_point`, or the
   marker at `new_path` matches a *different* device id that exists in
   `storage_devices`. Error message names the conflicting device label.
3. Write `.filemanagerid` containing `device_id` at `new_path` (adopt/overwrite —
   user already confirmed in the modal).
4. `db::update_device_mount_point(pool, device_id, new_path)` — new fn in `db.rs`:
   `UPDATE storage_devices SET mount_point = ?, last_seen = now WHERE id = ?`.
   Also refresh `available_bytes` via `devices::disk_space_for_path` (total stays 0
   for manually-pointed paths, consistent with `add_location`).
5. Return updated `StorageDevice` (fetch via `get_all_devices`, find by id).

Register both in `lib.rs` invoke handler.

## Frontend (TS/React)

- `src/types/index.ts`: `ReconnectCheck` type.
- `src/api/commands.ts`: `checkReconnectTarget(deviceId, path)`,
  `reconnectDevice(deviceId, path)`.
- `DeviceCard.tsx`: "Reconnect" button rendered only when `!device.isConnected`.
  Calls `onReconnect(device)`.
- `Devices.tsx`: handler opens tauri folder picker (`open({ directory: true })` from
  `@tauri-apps/plugin-dialog`, same as AddLocationModal pattern), then
  `checkReconnectTarget`, stores result + device in state, shows modal.
- New `src/components/ReconnectModal.tsx`:
  - Shows device label, old path → new path.
  - Marker status line: match → green "ID marker matches"; missing → yellow "No ID
    marker at this location"; mismatch → red "Path belongs to a different device".
  - Sample line: "N of M known files found at this location" (yellow/red if low).
  - Buttons: "Connect" (always enabled — force semantics; label "Connect anyway" when
    not a clean match) and "Cancel".
  - Connect → `reconnectDevice` → close modal, `refresh()`. Errors shown in modal.

## Error handling

- Offline/unreachable path → command error, shown in modal or page error area.
- Conflict with another registered device → command error, names the device.
- Marker write failure (read-only fs) → command error. `mount_point` NOT updated
  (write marker before DB update).
- Zero sampled files possible for never-scanned device → show "no indexed files to
  verify" instead of "0 of 0 found".

## Testing

Manual:
1. Add location, scan, rename folder → device disconnected → Reconnect to renamed
   folder → marker match, files found → connected, FileBrowser paths resolve.
2. Point at wrong folder → mismatch/missing + low sample count warning → cancel.
3. Force-connect onto unmarked copy of the data → marker written, works.
4. Point at another registered device's mount → rejected.

Then `cargo build` + create dmg (project rule).

## Out of scope

- Network drive re-pointing.
- Auto-reconnect improvements beyond existing `/Volumes` scan.
- Rescan-after-reconnect (user can trigger scan manually).

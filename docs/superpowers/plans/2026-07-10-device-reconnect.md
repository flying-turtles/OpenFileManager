# Device Reconnect Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** "Reconnect" action on disconnected devices in the Devices tab — pick a new folder, validate it's the same device (id marker + sampled known files), update `storage_devices.mount_point` so all `file_locations` rows (stored relative to mount point) stay valid.

**Architecture:** Two Tauri commands: `check_reconnect_target` (read-only validation, returns marker status + sample-file counts) and `reconnect_device` (writes `.filemanagerid` marker, updates DB row). Frontend: folder picker → check → confirm modal → commit → refresh.

**Tech Stack:** Rust (tauri 2, sqlx/sqlite, tokio), React 19 + TypeScript, `@tauri-apps/plugin-dialog`.

Spec: `docs/superpowers/specs/2026-07-10-device-reconnect-design.md`

## Global Constraints

- NEVER call sync `.exists()` / `std::fs` directly in async command bodies on user-supplied paths — network mounts stall. Use `super::path_online_within(path, 5)` first, then wrap fs work in `tokio::task::spawn_blocking` (pattern: `src-tauri/src/commands/mod.rs:39-54`).
- All serialized structs use `#[serde(rename_all = "camelCase")]` (models.rs convention).
- `file_locations.file_path` is RELATIVE to the device's `mount_point`.
- Marker file name: `devices::FILEMANAGER_ID_FILE` (`.filemanagerid`).
- Errors: `AppError::General(String)` (see `src-tauri/src/error.rs`).
- Commit after each task.
- After all tasks: build the dmg (project rule in CLAUDE.md): `npm run tauri build`.

---

### Task 1: DB helpers — `update_device_mount_point` + `get_device_file_sample`

**Files:**
- Modify: `src-tauri/src/db.rs` (device queries section, after `delete_device` ~line 126)
- Test: same file, `#[cfg(test)] mod tests` at bottom

**Interfaces:**
- Produces: `db::update_device_mount_point(pool, device_id: &str, mount_point: &str, available_bytes: i64) -> Result<(), AppError>`
- Produces: `db::get_device_file_sample(pool, device_id: &str, limit: i64) -> Result<Vec<FileLocation>, AppError>`

- [ ] **Step 1: Write failing test**

At the bottom of `src-tauri/src/db.rs` add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DetectedDisk;

    async fn test_pool() -> DbPool {
        let dir = std::env::temp_dir().join(format!("ofm-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let pool = init_pool(&dir.join("test.db")).await.unwrap();
        run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn update_mount_point_updates_row() {
        let pool = test_pool().await;
        let disk = DetectedDisk {
            id: "dev-1".into(),
            label: "Test".into(),
            mount_point: "/Volumes/Old".into(),
            total_bytes: 100,
            available_bytes: 50,
            is_removable: false,
        };
        upsert_device(&pool, &disk).await.unwrap();

        update_device_mount_point(&pool, "dev-1", "/Volumes/New", 42)
            .await
            .unwrap();

        let dev = get_device(&pool, "dev-1").await.unwrap();
        assert_eq!(dev.mount_point, "/Volumes/New");
        assert_eq!(dev.available_bytes, 42);
        assert_eq!(dev.total_bytes, 0); // manual re-point resets unreliable total
    }

    #[tokio::test]
    async fn file_sample_limited() {
        let pool = test_pool().await;
        let disk = DetectedDisk {
            id: "dev-1".into(),
            label: "Test".into(),
            mount_point: "/Volumes/Old".into(),
            total_bytes: 0,
            available_bytes: 0,
            is_removable: false,
        };
        upsert_device(&pool, &disk).await.unwrap();
        for i in 0..5 {
            upsert_file(&pool, &format!("hash-{i}"), 10, "f", "jpg").await.unwrap();
            upsert_location(
                &pool,
                &format!("hash-{i}"),
                "dev-1",
                &format!("sub/file-{i}.jpg"),
                &format!("file-{i}.jpg"),
                10,
                Some("2026-01-01T00:00:00Z"),
                "full",
            )
            .await
            .unwrap();
        }
        let sample = get_device_file_sample(&pool, "dev-1", 3).await.unwrap();
        assert_eq!(sample.len(), 3);
        let all = get_device_file_sample(&pool, "dev-1", 20).await.unwrap();
        assert_eq!(all.len(), 5);
    }
}
```

(Signatures verified against `src-tauri/src/db.rs:130` (`upsert_file`) and `db.rs:145` (`upsert_location`, `modified_at: Option<&str>`).)

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml update_mount_point file_sample`
Expected: compile error — `update_device_mount_point` / `get_device_file_sample` not found.

- [ ] **Step 3: Implement**

In `src-tauri/src/db.rs`, after `delete_device` (~line 126):

```rust
/// Re-point a device at a new mount path. Resets total_bytes to 0 —
/// totals are unreliable for manually-pointed paths (same as add_location).
pub async fn update_device_mount_point(
    pool: &DbPool,
    device_id: &str,
    mount_point: &str,
    available_bytes: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE storage_devices
         SET mount_point = ?, total_bytes = 0, available_bytes = ?, last_seen = datetime('now')
         WHERE id = ?",
    )
    .bind(mount_point)
    .bind(available_bytes)
    .bind(device_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_device_file_sample(
    pool: &DbPool,
    device_id: &str,
    limit: i64,
) -> Result<Vec<FileLocation>, AppError> {
    let rows = sqlx::query_as::<_, FileLocation>(
        "SELECT * FROM file_locations WHERE device_id = ? ORDER BY id LIMIT ?",
    )
    .bind(device_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml update_mount_point file_sample`
Expected: `test result: ok. 2 passed`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db.rs
git commit -m "db: update_device_mount_point + get_device_file_sample"
```

---

### Task 2: Marker evaluation helper + `ReconnectCheck` model

**Files:**
- Modify: `src-tauri/src/devices.rs` (append at end)
- Modify: `src-tauri/src/models.rs` (append)

**Interfaces:**
- Consumes: nothing new
- Produces: `devices::evaluate_marker(contents: Option<&str>, device_id: &str) -> (String, Option<String>)` — returns `("match"|"mismatch"|"missing", foreign_id)`
- Produces: `models::ReconnectCheck { marker_status: String, foreign_id: Option<String>, found_files: i64, sampled_files: i64 }`

- [ ] **Step 1: Write failing test**

At end of `src-tauri/src/devices.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_match() {
        assert_eq!(
            evaluate_marker(Some("dev-1\n"), "dev-1"),
            ("match".to_string(), None)
        );
    }

    #[test]
    fn marker_mismatch_reports_foreign_id() {
        assert_eq!(
            evaluate_marker(Some("other-id"), "dev-1"),
            ("mismatch".to_string(), Some("other-id".to_string()))
        );
    }

    #[test]
    fn marker_missing_or_empty() {
        assert_eq!(evaluate_marker(None, "dev-1"), ("missing".to_string(), None));
        assert_eq!(
            evaluate_marker(Some("   \n"), "dev-1"),
            ("missing".to_string(), None)
        );
    }
}
```

- [ ] **Step 2: Run test, verify fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml marker_`
Expected: compile error — `evaluate_marker` not found.

- [ ] **Step 3: Implement**

In `src-tauri/src/devices.rs` (before the test module):

```rust
/// Classify a `.filemanagerid` marker read from a reconnect target.
/// Returns ("match" | "mismatch" | "missing", foreign_id_if_mismatch).
pub fn evaluate_marker(contents: Option<&str>, device_id: &str) -> (String, Option<String>) {
    match contents.map(|c| c.trim().to_string()) {
        Some(id) if id.is_empty() => ("missing".into(), None),
        Some(id) if id == device_id => ("match".into(), None),
        Some(id) => ("mismatch".into(), Some(id)),
        None => ("missing".into(), None),
    }
}
```

In `src-tauri/src/models.rs` (append):

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectCheck {
    pub marker_status: String, // "match" | "mismatch" | "missing"
    pub foreign_id: Option<String>,
    pub found_files: i64,
    pub sampled_files: i64,
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml marker_`
Expected: `3 passed`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/devices.rs src-tauri/src/models.rs
git commit -m "devices: evaluate_marker helper + ReconnectCheck model"
```

---

### Task 3: Commands `check_reconnect_target` + `reconnect_device`, register in lib.rs

**Files:**
- Modify: `src-tauri/src/commands/devices.rs` (append after `add_location`)
- Modify: `src-tauri/src/lib.rs` (invoke_handler list, next to `commands::add_location`)

**Interfaces:**
- Consumes: `db::get_device_file_sample`, `db::update_device_mount_point`, `db::get_all_devices`, `db::get_device` (Task 1), `devices::evaluate_marker`, `models::ReconnectCheck` (Task 2), `super::path_online_within` (`commands/mod.rs:45`), `devices::disk_space_for_path`, `devices::FILEMANAGER_ID_FILE`
- Produces: tauri commands `check_reconnect_target(device_id: String, new_path: String) -> ReconnectCheck` and `reconnect_device(device_id: String, new_path: String) -> StorageDevice` (frontend invokes with `{ deviceId, newPath }`)

No unit test — commands need tauri `State`; logic extracted to tested helpers (Tasks 1–2). Verify via `cargo check` + manual test in Task 6.

- [ ] **Step 1: Implement both commands**

Append to `src-tauri/src/commands/devices.rs`:

```rust
#[tauri::command]
pub async fn check_reconnect_target(
    state: State<'_, AppState>,
    device_id: String,
    new_path: String,
) -> Result<ReconnectCheck, AppError> {
    if !super::path_online_within(&new_path, 5).await {
        return Err(AppError::General(format!("Path is not reachable: {}", new_path)));
    }

    let marker_path = std::path::Path::new(&new_path).join(devices::FILEMANAGER_ID_FILE);
    let marker = tokio::task::spawn_blocking(move || std::fs::read_to_string(marker_path).ok())
        .await
        .unwrap_or(None);
    let (marker_status, foreign_id) = devices::evaluate_marker(marker.as_deref(), &device_id);

    let sample = db::get_device_file_sample(&state.pool, &device_id, 20).await?;
    let sampled_files = sample.len() as i64;
    let base = std::path::PathBuf::from(&new_path);
    let found_files = tokio::task::spawn_blocking(move || {
        sample
            .iter()
            .filter(|f| base.join(&f.file_path).is_file())
            .count() as i64
    })
    .await
    .unwrap_or(0);

    Ok(ReconnectCheck {
        marker_status,
        foreign_id,
        found_files,
        sampled_files,
    })
}

#[tauri::command]
pub async fn reconnect_device(
    state: State<'_, AppState>,
    device_id: String,
    new_path: String,
) -> Result<StorageDevice, AppError> {
    if !super::path_online_within(&new_path, 5).await {
        return Err(AppError::General(format!("Path is not reachable: {}", new_path)));
    }

    let all = db::get_all_devices(&state.pool).await?;
    if let Some(other) = all
        .iter()
        .find(|d| d.id != device_id && d.mount_point == new_path)
    {
        return Err(AppError::General(format!(
            "Path is already used by device \"{}\"",
            other.label
        )));
    }

    // Marker belonging to a DIFFERENT registered device → hard reject
    let marker_path = std::path::Path::new(&new_path).join(devices::FILEMANAGER_ID_FILE);
    let mp = marker_path.clone();
    let marker = tokio::task::spawn_blocking(move || std::fs::read_to_string(mp).ok())
        .await
        .unwrap_or(None);
    if let Some(m) = marker.map(|s| s.trim().to_string()) {
        if !m.is_empty() && m != device_id {
            if let Some(other) = all.iter().find(|d| d.id == m) {
                return Err(AppError::General(format!(
                    "Folder is marked as device \"{}\" — remove that device first or pick another folder",
                    other.label
                )));
            }
        }
    }

    // Adopt: write marker BEFORE touching the DB, so a read-only fs aborts cleanly
    let id_clone = device_id.clone();
    let wp = marker_path.clone();
    tokio::task::spawn_blocking(move || std::fs::write(&wp, &id_clone))
        .await
        .map_err(|e| AppError::General(e.to_string()))?
        .map_err(|e| AppError::General(format!("Failed to write id marker: {}", e)))?;

    let path_clone = new_path.clone();
    let (_, avail) =
        tokio::task::spawn_blocking(move || devices::disk_space_for_path(&path_clone))
            .await
            .unwrap_or((0, 0));

    db::update_device_mount_point(&state.pool, &device_id, &new_path, avail).await?;
    db::get_device(&state.pool, &device_id).await
}
```

- [ ] **Step 2: Register in lib.rs**

In `src-tauri/src/lib.rs`, in the `tauri::generate_handler![` list directly after `commands::add_location,` add:

```rust
            commands::check_reconnect_target,
            commands::reconnect_device,
```

- [ ] **Step 3: Verify compile + all tests**

Run: `cargo check --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: check clean; all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/devices.rs src-tauri/src/lib.rs
git commit -m "commands: check_reconnect_target + reconnect_device"
```

---

### Task 4: Frontend types + API wrappers

**Files:**
- Modify: `src/types/index.ts` (append)
- Modify: `src/api/commands.ts` (after `removeDevice`, ~line 58)

**Interfaces:**
- Consumes: Task 3 command names/args.
- Produces: `ReconnectCheck` type; `checkReconnectTarget(deviceId, newPath): Promise<ReconnectCheck>`; `reconnectDevice(deviceId, newPath): Promise<StorageDevice>`

- [ ] **Step 1: Add type**

In `src/types/index.ts` append:

```ts
export interface ReconnectCheck {
  markerStatus: "match" | "mismatch" | "missing";
  foreignId: string | null;
  foundFiles: number;
  sampledFiles: number;
}
```

- [ ] **Step 2: Add API wrappers**

In `src/api/commands.ts` after `removeDevice` (~line 58) add (and add `ReconnectCheck` to the type import list at the top):

```ts
export async function checkReconnectTarget(
  deviceId: string,
  newPath: string
): Promise<ReconnectCheck> {
  return invoke("check_reconnect_target", { deviceId, newPath });
}

export async function reconnectDevice(
  deviceId: string,
  newPath: string
): Promise<StorageDevice> {
  return invoke("reconnect_device", { deviceId, newPath });
}
```

- [ ] **Step 3: Typecheck**

Run: `npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/types/index.ts src/api/commands.ts
git commit -m "frontend: reconnect API wrappers + ReconnectCheck type"
```

---

### Task 5: `ReconnectModal` component

**Files:**
- Create: `src/components/ReconnectModal.tsx`

**Interfaces:**
- Consumes: `ReconnectCheck`, `StorageDevice` types; `reconnectDevice` wrapper (Task 4); `useFocusTrap` hook (existing, see `AddLocationModal.tsx:3`); modal CSS classes `modal-overlay`/`modal-content`/`form-actions`/`error-msg`/`success-msg` (existing global styles).
- Produces: `<ReconnectModal device path check onDone onClose />` — `onDone()` called after successful reconnect (caller refreshes + closes), `onClose()` on cancel.

- [ ] **Step 1: Create component**

```tsx
import { useState } from "react";
import type { ReconnectCheck, StorageDevice } from "../types";
import { reconnectDevice } from "../api/commands";
import { useFocusTrap } from "../hooks/useFocusTrap";

interface Props {
  device: StorageDevice;
  path: string;
  check: ReconnectCheck;
  onDone: () => void;
  onClose: () => void;
}

export function ReconnectModal({ device, path, check, onDone, onClose }: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  const cleanMatch = check.markerStatus === "match";
  const markerLine =
    check.markerStatus === "match"
      ? "✓ ID marker matches this device"
      : check.markerStatus === "missing"
        ? "⚠ No ID marker at this location — it will be created"
        : `✕ Folder is marked as a different device (${check.foreignId})`;
  const sampleLine =
    check.sampledFiles === 0
      ? "No indexed files to verify (device was never scanned)"
      : `${check.foundFiles} of ${check.sampledFiles} known files found at this location`;
  const sampleOk = check.sampledFiles === 0 || check.foundFiles === check.sampledFiles;

  const handleConnect = async () => {
    setSubmitting(true);
    setError("");
    try {
      await reconnectDevice(device.id, path);
      onDone();
    } catch (e) {
      setError(String(e));
      setSubmitting(false);
    }
  };

  return (
    <div
      className="modal-overlay"
      onClick={onClose}
      role="dialog"
      aria-label="Reconnect device"
      aria-modal="true"
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      <div className="modal-content" ref={trapRef} onClick={(e) => e.stopPropagation()}>
        <h2>Reconnect {device.label}</h2>
        {error && <div className="error-msg">{error}</div>}
        <div className="form-group">
          <label>Old path</label>
          <div className="device-mount">{device.mountPoint}</div>
        </div>
        <div className="form-group">
          <label>New path</label>
          <div className="device-mount">{path}</div>
        </div>
        <div className={cleanMatch ? "success-msg" : "error-msg"}>{markerLine}</div>
        <div className={sampleOk ? "success-msg" : "error-msg"}>{sampleLine}</div>
        <div className="form-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="btn-primary" onClick={handleConnect} disabled={submitting}>
            {submitting
              ? "Connecting..."
              : cleanMatch && sampleOk
                ? "Connect"
                : "Connect anyway"}
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Typecheck**

Run: `npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/ReconnectModal.tsx
git commit -m "frontend: ReconnectModal"
```

---

### Task 6: Wire up — DeviceCard button + Devices page flow

**Files:**
- Modify: `src/components/DeviceCard.tsx`
- Modify: `src/pages/Devices.tsx`

**Interfaces:**
- Consumes: `ReconnectModal` (Task 5), `checkReconnectTarget` (Task 4), `open` from `@tauri-apps/plugin-dialog` (already a dependency).
- Produces: `DeviceCard` prop `onReconnect?: (device: StorageDevice) => void`.

- [ ] **Step 1: DeviceCard — add prop + button**

In `src/components/DeviceCard.tsx`:

Add to `Props` interface (after `onVerify`):

```ts
  onReconnect?: (device: StorageDevice) => void;
```

Add `onReconnect` to the destructured params:

```tsx
export function DeviceCard({ device, onSetType, onSetSpeed, onScan, onVerify, verifyDisabled, onRemove, onReconnect }: Props) {
```

In the `device-actions` div, directly before the Remove button:

```tsx
        {!device.isConnected && onReconnect && (
          <button
            onClick={() => onReconnect(device)}
            title="Point this device at a new folder — indexed files stay valid"
          >
            Reconnect
          </button>
        )}
```

- [ ] **Step 2: Devices.tsx — picker, check, modal**

In `src/pages/Devices.tsx`:

Add imports:

```tsx
import { open } from "@tauri-apps/plugin-dialog";
import type { ReconnectCheck } from "../types";
import { ReconnectModal } from "../components/ReconnectModal";
import { addLocation, checkReconnectTarget } from "../api/commands";
```

(replaces the existing `import { addLocation } from "../api/commands";` line)

Add state + handler inside the component (after `showAddNetworkModal` state):

```tsx
  const [reconnectTarget, setReconnectTarget] = useState<{
    device: StorageDevice;
    path: string;
    check: ReconnectCheck;
  } | null>(null);
  const [reconnectError, setReconnectError] = useState("");

  const handleReconnect = async (device: StorageDevice) => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;
    setReconnectError("");
    try {
      const check = await checkReconnectTarget(device.id, selected);
      setReconnectTarget({ device, path: selected, check });
    } catch (e) {
      setReconnectError(String(e));
    }
  };
```

Render the error above the device sections (next to the existing verify error block):

```tsx
      {reconnectError && (
        <div className="error-msg">
          <div className="flex-row gap-8" style={{ justifyContent: "space-between", alignItems: "center" }}>
            <span>{reconnectError}</span>
            <button onClick={() => setReconnectError("")}>Dismiss</button>
          </div>
        </div>
      )}
```

Pass the handler to `DeviceCard` (add prop alongside `onRemove`):

```tsx
                  onReconnect={handleReconnect}
```

Render the modal at the bottom (next to the other modals):

```tsx
      {reconnectTarget && (
        <ReconnectModal
          device={reconnectTarget.device}
          path={reconnectTarget.path}
          check={reconnectTarget.check}
          onDone={() => {
            setReconnectTarget(null);
            refresh();
          }}
          onClose={() => setReconnectTarget(null)}
        />
      )}
```

- [ ] **Step 3: Typecheck + build frontend**

Run: `npx tsc --noEmit && npm run build`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/components/DeviceCard.tsx src/pages/Devices.tsx
git commit -m "devices tab: Reconnect flow for disconnected devices"
```

---

### Task 7: End-to-end verification + dmg

**Files:** none (verification only)

- [ ] **Step 1: Full test suite + build**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && npx tsc --noEmit`
Expected: all pass.

- [ ] **Step 2: Manual verification (dev app)**

Run: `npm run tauri dev`, then:

1. Add Location on a temp folder with a few files (e.g. `~/reconnect-test/a`), scan it.
2. Quit nothing; rename folder to `~/reconnect-test/b` → hit Refresh → device shows Disconnected.
3. Click Reconnect → pick `~/reconnect-test/b` → modal shows "ID marker matches" + "N of N known files found" → Connect → device connected, mount path updated, FileBrowser resolves files.
4. Repeat with a wrong folder → modal shows warning states → Cancel works.
5. Pick another registered device's folder → error names conflicting device.

Expected: all five behaviors as described.

- [ ] **Step 3: Build dmg (project rule)**

Run: `npm run tauri build`
Expected: dmg produced under `src-tauri/target/release/bundle/dmg/`.

- [ ] **Step 4: Commit any leftovers**

```bash
git status
# commit only intentional changes; plan checkboxes updated
```

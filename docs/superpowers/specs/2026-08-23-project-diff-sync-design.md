# Project Diff & Sync — Design

Date: 2026-08-23

## Goal

After a project's files have been imported to several devices, one of those
devices becomes the working copy — the disk Lightroom edits from. Rejecting
photos in Lightroom deletes them from that disk only, so the backups drift
ahead of it, holding files the photographer has decided not to keep.

This feature lets the user nominate one device as the **source of truth**
(SoT) for a project, see how every other device differs from it, and resolve
that difference in both directions: delete the rejected files from the
backups, and copy across anything the backups are still missing.

## Scope

Entry point is the Projects screen only. Generalizing to arbitrary day folders
is explicitly out of scope; the engine is written so a second entry point can
reuse it later.

## Decisions

| Question | Decision |
|---|---|
| Scope of a diff | One project, selected in the Projects screen |
| Deletion candidates | Only files the index says were on the SoT and that a live check confirms are gone from disk. Files that never lived on the SoT are never deletion candidates. |
| Other direction | Files present on the SoT but missing from a backup are shown and can be copied |
| Participating backups | Every reachable device the index says holds project files; unreachable ones are listed as skipped |
| Copy target path | Mirror of the SoT relative path; an occupied target is skipped, never overwritten |
| Deletion mechanism | Existing `bulk_delete_file_copies` — Trash by default, permanent via opt-in |
| Identity | `(blake3_hash, file_size)`, never hash alone |

## Why a live filesystem check

A project's file set is derived from the index (`db::get_project_files`,
date range over `MIN(modified_at)`). When Lightroom deletes a file from the
SoT disk, the index still holds that `file_locations` row. A DB-only diff
would therefore show nothing at all — the very files the user wants to sync
are invisible to it.

So the index supplies the *candidate set* and the *before* state, and a live
`stat` of each candidate path supplies the *after* state. No hashing is
involved; the diff is a metadata pass.

Two alternatives were rejected:

- **Rescan the SoT device first, then diff in SQL.** Correct, and it reuses
  the scanner, but project files are scattered across day folders, so it means
  rescanning (and rehashing) a whole disk before every diff. Too slow for a
  per-shoot workflow.
- **Walk day folders on each device and compare relative paths.** Project
  membership comes from the index in the first place, so there is no folder
  set to walk without it. Redundant.

## Architecture

New module `src-tauri/src/diff.rs` (engine) and command file
`src-tauri/src/commands/diff.rs`. New page `src/pages/ProjectDiff.tsx` with
`ProjectDiff.css` and hook `src/hooks/useProjectDiff.ts`.

The engine reuses existing pieces rather than growing its own: `db::get_project_files`,
`commands::path_online` / `path_online_within`, `importer::copy_file_cancellable`,
`db::upsert_file` / `upsert_location` / `delete_file_location_no_cleanup` /
`cleanup_orphaned_files`, and the whole `bulk_delete_file_copies` command for
the deletion half.

## Commands

### `get_project_diff_devices(project_id) -> Vec<DiffDeviceOption>`

Distinct devices across all locations of the project's files, each with
`device_id`, `label`, `file_count`, `is_connected`. Feeds the source-of-truth
dropdown. Disconnected devices are returned and marked, not filtered out — the
user should see that a disk is offline rather than wonder where it went.

### `compute_project_diff(project_id, sot_device_id, on_event) -> ProjectDiff`

Cancellable, reports progress on a channel.

Preconditions, checked before any work:

- The SoT device exists and its mount point passes `path_online_within(5)`.
  A sleeping NAS gets a chance to spin up; an absent disk is a hard error.
- The SoT device holds at least one project file per the index.

For each project file, grouped by `(blake3_hash, file_size)`:

1. `stat` the file's indexed path on the SoT, if it has one.
2. `stat` the file's indexed path on each reachable backup device.

Each `stat` runs through `tokio::task::spawn_blocking` behind a timeout and a
concurrency semaphore, so a stalled network mount cannot occupy the runtime.
The SoT mount is re-checked periodically during the pass; if it drops, the
whole run aborts with an error rather than reporting every remaining file as
vanished.

Classification:

| SoT state | Backup state | Result |
|---|---|---|
| Indexed, `NotFound` on disk | file present | **delete candidate** on that backup |
| Indexed, `NotFound` on disk | absent | nothing (already in sync) |
| Present on disk | no row, or row whose file is `NotFound` | **copy candidate** for that backup |
| Present on disk | present | in sync |
| Never indexed on SoT | any | ignored — never a delete candidate |
| Any non-`NotFound` error | — | excluded from both lists, counted in `unreadable` |

The `unreadable` bucket is what keeps a flaky mount from turning into data
loss: only an unambiguous `NotFound` is read as "the user deleted this".
Timeouts, permission errors and IO errors take the file out of the diff
entirely and surface as a warning in the UI.

Every vanished SoT file also yields a **purge candidate** — its stale
`file_locations` row. These are not deleted during compute. A compute run is
read-only against the index, so a misread cannot silently mutate it; the purge
happens at resolve time, after the user has confirmed.

### `copy_diff_files(items, on_event) -> DiffCopyResult`

Each item carries the source absolute path, the target device id, and the
mirrored relative path. Per file: create the target directory, copy via
`importer::copy_file_cancellable`, then `db::upsert_file` + `db::upsert_location`.
An existing file at the target is skipped and reported — this feature never
overwrites. Failures are collected per file; the run continues.

### Deletion

No new command. The frontend passes the selected backup location ids to the
existing `bulk_delete_file_copies`, which already trashes by default, honours
a `permanent` flag, removes the rows, and returns a per-file failure list.

### Purging stale SoT rows

After a successful resolve, `delete_file_location_no_cleanup` for each purge
candidate, then one `cleanup_orphaned_files`. A file whose only remaining
copies were just deleted from the backups disappears from the index entirely,
which is correct — it no longer exists anywhere.

Purging is unconditional on the delete selection. A row pointing at a file
that is gone from disk is wrong regardless of whether the user chose to
propagate that deletion to the backups.

## Types

Rust in `models.rs`, mirrored in `src/types/index.ts` (camelCase, as elsewhere):

```rust
pub struct DiffDeviceOption {
    pub device_id: String,
    pub label: String,
    pub file_count: i64,
    pub is_connected: bool,
}

pub struct DiffFileEntry {
    pub blake3_hash: String,
    pub file_size: i64,
    pub file_name: String,
    /// Path relative to the mount of the device this entry belongs to. For a
    /// copy entry that is the target device, and the value is the mirrored
    /// SoT relative path.
    pub relative_path: String,
    /// Backup location row to delete. None for copy entries.
    pub location_id: Option<i64>,
    /// Absolute source path. Set for copy entries only.
    pub source_path: Option<String>,
}

pub struct DiffDeviceResult {
    pub device_id: String,
    pub device_label: String,
    pub skip_reason: Option<String>,
    pub to_delete: Vec<DiffFileEntry>,
    pub to_copy: Vec<DiffFileEntry>,
    pub delete_bytes: i64,
    pub copy_bytes: i64,
}

pub struct ProjectDiff {
    pub project_id: i64,
    pub sot_device_id: String,
    pub sot_label: String,
    pub devices: Vec<DiffDeviceResult>,
    /// Stale SoT rows, purged on resolve.
    pub purge_location_ids: Vec<i64>,
    pub unreadable: Vec<DiffUnreadable>,
    pub total_delete_files: i64,
    pub total_delete_bytes: i64,
    pub total_copy_files: i64,
    pub total_copy_bytes: i64,
}

pub struct DiffUnreadable {
    pub device_id: String,
    pub relative_path: String,
    pub error: String,
}

pub struct DiffCopyError {
    pub file_name: String,
    pub target_device_id: String,
    pub error: String,
}

pub struct DiffCopyResult {
    pub copied: i64,
    pub bytes_copied: i64,
    /// Targets that already held a file at the mirrored path.
    pub skipped: Vec<DiffCopyError>,
    pub failed: Vec<DiffCopyError>,
}
```

Events:

```rust
pub enum DiffEvent {
    Started { total: u64 },
    Progress { checked: u64, total: u64, current_device: String },
    Finished,
    Error { message: String },
    Cancelled,
}

pub enum DiffCopyEvent {
    Progress(DeviceCopyProgress),
    Complete(DiffCopyResult),
    Error { message: String },
    Cancelled,
}
```

`ProjectDiff` crosses IPC in full, unlike `MovePlan`. A shoot is on the order
of a few thousand files, so the payload is a few hundred kilobytes, and the UI
needs every entry anyway to render checkboxes and thumbnails. Move's reason
for keeping its plan in `AppState` — a folder tree of tens of thousands of
files that the UI never displays — does not apply.

## Frontend

### Projects screen

The project detail view gains a **Source of truth** `<select>` listing
`get_project_diff_devices` results, offline devices disabled and labelled as
such, and a **Show diff** button enabled once a device is chosen.

### ProjectDiff page

Reached through App.tsx page state, the same handoff Projects already uses for
Transfer. Not a sidebar entry. A Back button returns to the project.

Layout:

- Header: project title, SoT device label, totals (files and bytes to delete,
  files and bytes to copy), and a warning strip when `unreadable` is non-empty
  or a device was skipped.
- Two sections, **Delete from backups** and **Copy to backups**, each grouping
  device → folder → file. Checkboxes at every level, so a whole subfolder can
  be deselected in one click; a folder checkbox is indeterminate when its
  children are mixed.
- Delete entries are checked by default, per the requirement that the
  backups sync to the SoT unless the user intervenes. Copy entries are checked
  by default too; the two sections resolve independently.
- Rows are virtualized with `@tanstack/react-virtual`, as in `FileTable`.

### Thumbnails

Each row shows a small preview via the existing `<FilePreview>`, which already
handles RAW, HEIC and video through `getThumbnail`.

`FilePreview` mounts only when its row scrolls into view. Mounting all of them
would kick off a thumbnail generation per file, which for a few thousand
files means an unusable page. Virtualization covers the list rows; the
confirmation grid uses an `IntersectionObserver` wrapper for the same reason.

Previews always resolve from a copy that still exists: backup locations for
delete entries (the SoT copy is gone by definition), the SoT location for copy
entries (the backup copy does not exist yet).

### Resolve dialog

House-style modal. Shows per-device counts, the shared `PermanentToggle`, and
a scrollable thumbnail grid of exactly the files about to be deleted, so the
user can eyeball the rejects before committing. Confirm runs deletions first,
then copies, then the SoT row purge, with a progress bar per phase and a
per-file failure list at the end.

Deleting to the Trash is recoverable, so it needs no typed confirmation.
Permanent deletion follows `SourceCleanupModal`'s precedent and requires
typing the SoT device label.

## Error handling

| Situation | Behaviour |
|---|---|
| SoT mount offline at start | Hard error, diff refuses to run |
| SoT mount drops mid-check | Run aborts with an error; no partial diff shown |
| Backup device unreachable | Listed with a `skip_reason`; never treated as "files missing" |
| `stat` returns anything but `NotFound` | File excluded from both lists, added to `unreadable`, surfaced in the warning strip |
| Copy target already occupied | Skipped and reported; never overwritten |
| Individual delete or copy failure | Collected per file, run continues, listed in the result |
| User cancels | Compute and copy both honour a `CancellationToken`; a cancelled copy removes its partial file |

## Testing

Rust `#[cfg(test)]` in `diff.rs`, over tempdirs, following `mover.rs`:

- A file indexed on the SoT but absent from disk becomes a delete candidate on
  each backup that still has it.
- A file that was never indexed on the SoT never becomes a delete candidate.
- Two files sharing a `blake3_hash` but differing in size are not treated as
  the same file.
- A `stat` failure that is not `NotFound` excludes the file from both lists
  and lands in `unreadable`.
- The mirrored copy path equals the SoT relative path under the backup mount.
- An occupied copy target is skipped, and the existing file is untouched.
- Resolve moves backup copies to the Trash rather than unlinking them, and
  purges the stale SoT rows.

There is no frontend test runner in this repo. The tree-grouping and
checkbox-propagation logic therefore lives in pure exported helpers in
`useProjectDiff.ts`, small enough to review by reading.

## Out of scope

- Diffing arbitrary folders or dates outside a project.
- Three-way merges or conflict resolution between two backups that disagree
  with each other; every comparison is against the SoT.
- Re-hashing to detect files whose content changed in place. The diff is about
  presence and absence.

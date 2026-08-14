# Move Tab — Design

Date: 2026-08-14

## Goal

A `Move` tab that moves files and folders between locations. A move is only
complete when the data is verifiably at the destination, the source copy is
gone, and the index reflects the new location.

## Decisions

| Question | Decision |
|---|---|
| Source selection | Two-pane browser built on the existing `browse_directory` command |
| Verification | Full blake3 hash of the destination compared against the source |
| Source deletion | macOS Trash by default, permanent delete via opt-in checkbox |
| Destination layout | Date-based dirs (`<dest>/YYYY-MM-DD/<name>`), as in Import and Transfer |

Consequence of the layout decision: a moved folder's internal structure is not
preserved. `Shoot/RAW/x.cr3` lands at `<dest>/2026-03-14/x.cr3`. This matches
the rest of the app and was chosen deliberately.

## Architecture

New module `src-tauri/src/mover.rs`, new command file
`src-tauri/src/commands/move_files.rs`, new page `src/pages/Move.tsx`.

The move engine is separate from `transfer.rs`. Transfer resolves its work
from the database (`ResolvedTransferFile` built out of `FileSafety`); Move is
path-driven and must handle files the index has never seen. Sharing one
function would mean two source models inside it. Move reuses the smaller
pieces instead: `hasher`, `commands::files::remove_from_disk`,
`db::upsert_file` / `upsert_location` / `delete_location_by_device_and_path`,
and `commands::path_online_within`.

## Planning

`plan_move(sources: Vec<PathBuf>, dest: PathBuf) -> MovePlan`

- Walks each source with the same `WalkBuilder` configuration the importer
  uses (hidden files skipped, `.openfileignore` honored). A source may be a
  single file or a folder.
- Resolves the device for each source root and for the destination via
  `devices::device_for_path`. An unresolvable device is a hard error — without
  a device id the index cannot be updated.
- Computes each destination path as `<dest>/<YYYY-MM-DD>/<file_name>`, where
  the date comes from the file's modified time, falling back to `unknown`.
  Name collisions get `_1`, `_2`, … suffixes. Collisions are resolved against
  both the filesystem and the paths already assigned within this plan, so two
  same-named sources cannot be assigned the same destination.
- Sets `same_volume` per file: true when the source device id equals the
  destination device id.

Rejected up front, as errors:

- Destination is inside one of the source folders.
- Destination equals a source.
- Either root fails `path_online_within(5)`.

`MovePlan` carries the per-file list, total file count, total bytes, the
source and destination device ids and labels, and a `same_volume_count`.

## Execution

`run_move(plan, permanent, channel, cancel_token) -> MoveResult`

Per file, in order:

1. **Same volume** — `tokio::fs::rename`. No bytes cross a device boundary, so
   verification is existence plus a byte-size match against the pre-move
   metadata. Any rename failure (`EXDEV` from a boundary the device id did not
   predict, permissions, a filesystem that refuses it) falls through to the
   copy path rather than failing the file.
2. **Cross volume** — chunked copy (1 MB chunks, cancellable between chunks,
   `flush` then `sync_all` at the end), matching `transfer.rs`. The source
   bytes are fed to two blake3 hashers while streaming: one over the whole
   file, one over the first 4 MB. This yields both the verification hash and
   the index's partial hash from a single source read.
3. **Verify** — re-read the destination and compute its full blake3. Compare
   to the source's full hash. On mismatch: delete the destination copy, leave
   the source untouched, record a `MoveError` for that file, continue.
4. **Delete source** — `remove_from_disk(source, permanent)`. Only reached
   after verification succeeds.
5. **Index** — `delete_location_by_device_and_path(source_device, rel_path)`
   drops the old row (a no-op when the file was never indexed), then
   `upsert_file` and `upsert_location(..., source: "move")` record the new
   path, and `set_full_hash` stores the full hash computed during
   verification. `cleanup_orphaned_files` runs once after the whole run.

After all files, empty directories under each moved source folder are removed
bottom-up, and the source folder itself if it is empty. Directories that still
hold files (skipped hidden files, failures) are left alone.

Cancellation between chunks removes the partial destination file and leaves
the source untouched. A cancelled run reports what it completed.

Per-file failures never abort the run. They accumulate in
`MoveResult.failed` and are rendered in the result panel.

## Data model

```rust
MoveFile { source_path, dest_path, file_name, file_size, modified_at, same_volume }
MovePlan { files, total_files, total_bytes, source_device_id, dest_device_id,
           dest_label, same_volume_count }
MoveResult { moved, bytes_moved, failed: Vec<MoveError> }
MoveError { source_path, file_name, error }

MoveEvent
  PlanReady(MovePlan)
  Progress { processed, total, bytes_moved, total_bytes, current_file, phase }
  FileFailed(MoveError)
  Complete(MoveResult)
  Cancelled
```

`phase` is one of `copying`, `verifying`, `deleting`. Progress is throttled to
500 ms during a copy, as in `transfer.rs`.

## Commands

- `plan_move(sources, dest) -> MovePlan` — stores the plan in `AppState` and
  returns it.
- `start_move(permanent, on_event)` — runs the stored plan on a spawned task.
- `cancel_move()` — cancels the stored token.

`AppState` gains `move_cancel_token: Arc<Mutex<Option<CancellationToken>>>`
and `move_plan: Arc<Mutex<Option<Arc<MovePlan>>>>`, mirroring the import pair.

Filesystem reachability is checked with `path_online_within`, never a bare
`.exists()` in async command code — a stalled SMB mount blocks runtime workers
and starves the DB pool.

## Frontend

`src/pages/Move.tsx`, `src/pages/Move.css`, `src/hooks/useMove.ts`, and a nav
entry after Transfer in `src/App.tsx`.

Two panes. Each has a device dropdown, a breadcrumb path, and a listing from
`browse_directory`. The left pane has checkboxes on both files and folders;
the right pane navigates only and contributes the destination directory.

The middle column holds the `Move →` button, the permanent-delete checkbox,
and the plan summary once `plan_move` returns (file count, total size,
destination, and a note when the move is same-volume). Confirming runs
`start_move`.

Phases: `idle` → `planning` → `planned` → `moving` → `complete`. Progress
shows a bar, the current file, and the phase label. The result panel lists
moved count, bytes moved, and every failure with its reason.
Both panes refresh their listings on completion.

## Testing

Rust unit tests over the pure planning logic:

- Date-dir mapping from `modified_at`, including the `unknown` fallback.
- Dedup suffixes, including two plan entries competing for one name.
- Destination-inside-source rejection.
- Destination-equals-source rejection.
- `same_volume` detection.
- The verify decision function: matching hashes permit deletion, mismatched
  hashes do not.

Manual verification: same-volume move, cross-volume move, a single file, a
folder, cancel mid-run, and a destination that already holds a file of the
same name. Confirm afterwards that the index shows the new location and no
longer shows the old one.

## Out of scope

- Preserving source folder structure at the destination.
- Moving between a local disk and a network drive that is not a registered
  device.
- Undo. Trash is the recovery path for a wrong move.

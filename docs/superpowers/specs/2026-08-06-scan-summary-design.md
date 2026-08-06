# Scan Result Summary + Redundant-File Cleanup

Date: 2026-08-06

## Problem

A finished scan reports four numbers on one line (`scanned / hashed / added /
removed`). That says what the scan *did*, not what the scanned location
*contains*. Users cannot see how much data sits there, when the files are
from, whether they belong to a project, or which of them are already backed
up elsewhere and therefore safe to delete.

## Scope

After a scan completes, the Scanner tab shows a summary panel covering the
whole scanned location, plus an action to delete every file in it that has
copies on at least two other devices.

Out of scope: full-hash verification before delete, per-file selection,
summaries for locations that were not just scanned.

## Key design facts

**Summaries come from the DB, not from scan counters.** The scanner skips
unchanged directories and files (`scanner.rs` phase 1), so its counters
describe only what changed. The `file_locations` rows under
`(device_id, scan_prefix)` describe the full location and are already
up to date when the scan finishes.

**`blake3_hash` is a partial hash.** `hasher::hash_file_partial_sync` covers
only the first 4 MB. Two different files that share a header (common for
video containers and padded formats) collide. Every delete decision in this
feature therefore also requires an exact `file_size` match.

**Prefix matching needs a boundary.** The existing helpers use a bare
`LIKE 'prefix%'`, so scanning `Foo` also matches `Foobar`. Fixed as part of
this work (see below).

## Design

### Scan scope resolution

`scanner::resolve_scan_scope(target) -> ScanScope { device_id, mount_point,
scan_prefix }`, extracted from `run_scan` so the scan and the summary can
never disagree about what "the scanned location" is. `scan_prefix` is the
target path relative to the mount point; scanning a device root yields `""`.

### Path scope predicate (db.rs)

A shared predicate replaces the bare-prefix `LIKE` in
`get_locations_by_prefix`, `remove_stale_locations`, `get_dir_cache`, and
`remove_stale_dir_cache`:

- `scan_prefix == ""` → the whole device
- otherwise → `col = ? OR col LIKE ? ESCAPE '\'` with pattern `prefix/%`

`%`, `_`, and `\` are escaped in the pattern.

### `get_scan_summary(target) -> ScanSummary`

| Field | Source |
|---|---|
| `totalFiles`, `totalBytes` | `COUNT(*)`, `SUM(file_size)` over scoped locations |
| `oldestModified`, `newestModified` | `MIN/MAX(modified_at)` over scoped locations |
| `projects[]` | `{id, title, fileCount, totalBytes}` per project |
| `unassignedFiles` | scoped files matching no project |
| `redundantFiles`, `redundantBytes` | eligibility predicate below |

Project membership reuses the Projects page rule: a file belongs to a project
when `MIN(modified_at)` across *all* its locations falls in
`[start_date, end_date + 1 day)`. Counts are unique files (hashes), matching
that page.

### Eligibility predicate

A scoped location is deletable when:

```sql
(SELECT COUNT(DISTINCT o.device_id)
   FROM file_locations o
  WHERE o.blake3_hash = fl.blake3_hash
    AND o.device_id  <> fl.device_id
    AND o.file_size   = fl.file_size) >= 2
```

plus `blake3_hash NOT LIKE 'deferred:%'`.

`DISTINCT o.device_id` with `device_id <> fl.device_id` is what makes the two
other locations genuinely *other* — two copies elsewhere on the scanned drive
do not qualify, because one failing disk would take all copies with it.
Network drives get `storage_devices` rows (`network.rs`), so they count as
devices like any other.

`o.file_size = fl.file_size` is the partial-hash guard described above.

### Delete

`delete_redundant_scanned_files(target, permanent, on_event)`:

1. Recompute eligibility from the DB — the frontend list is never trusted.
2. `path_online_within(mount_point, 5)` before touching anything; network
   mounts stall rather than fail.
3. Per file: `remove_from_disk` (Trash by default, permanent opt-in) →
   `delete_location_by_device_and_path`.
4. `cleanup_orphaned_files` once at the end.

Reuses `SourceCleanupEvent` / `SourceCleanupPreview` / `SourceCleanupResult`.
`SourceCleanupPreview` gains `fileCount` (total eligible) so the preview list
can be truncated for display while the counts stay exact.

### Import cleanup backport

`importer::compute_source_cleanup` gets the same `file_size` guard. It has
the same partial-hash exposure today.

### Frontend

- `ScanSummaryPanel` — new component, replaces the one-line scan result:
  scan counters, total size and file count, date range, project chips
  (clickable → Projects page), and a delete button when `redundantFiles > 0`.
- `SourceCleanupModal` — generalized to take `loadPreview` / `runDelete` /
  `title` props so Import and Scanner share one modal instead of two copies.
  Preview renders at most 500 rows with a "showing first N of M" note.
- `App.tsx` — Scanner gains an `onOpenProject` callback; Projects gains an
  `initialProjectId` prop.

## Testing

Manual, against the real index:

1. Scan a folder with known duplicates → summary counts match the Files tab.
2. Scan a device root (`scan_prefix == ""`) → summary covers the whole device.
3. Sibling-directory check: with `Foo` and `Foobar` both indexed, scanning
   `Foo` must not report `Foobar`'s files.
4. A file with copies on exactly 1 other device → not eligible.
5. A file with 2 copies on the *same* other device → not eligible.
6. Same-hash different-size pair → not eligible.
7. Delete with an unplugged backup device → still eligible (eligibility is
   DB state, not reachability); deleting from an unreachable *source* fails
   the online check with a clear message.

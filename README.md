# OpenFileManager

A macOS desktop app for photographers and anyone else with files spread across many drives. It indexes every drive by content (BLAKE3), shows which files are safely backed up and which exist only once, and helps you import, copy, move and clean up files without losing anything.

## Features

- **Devices** — Detects mounted disks and volumes. Label each as **hot** (working) or **cold** (backup/archive). Add SMB network drives (credentials stored in the macOS Keychain). Reconnect a drive that reappears under a different path without rescanning.
- **Scanner** — Indexes a folder or whole drive, hashing every file. Scans can be paused and resumed. A summary shows what the location contains and can delete files that already have copies on at least two other devices.
- **Safety view** — A file counts as **safe** when it has at least two copies on at least two devices, one of them cold. Everything else is flagged **unsafe**.
- **Files** — Browse the index by device or list only unsafe files. Expand a file to see every copy. Thumbnails and full-size previews for images and videos.
- **Import** — Copy from an SD card or any folder into date-based folders (`YYYY-MM-DD/`), hashing and indexing as it goes.
- **Projects** — Group files by date range (e.g. a shoot or trip) and see how well each project is backed up.
- **Transfer** — Copy a project's files to another drive, skipping what's already there and reading from the fastest available source.
- **Move** — Move files between locations. The source is only removed (to the Trash by default) after the destination's full hash matches.
- **Project diff & sync** — Pick one drive as the source of truth for a project (e.g. the disk Lightroom edits from), see how other drives differ, delete rejected files from backups and copy across anything they're missing.
- **Similar** — Find near-duplicate photos and videos by perceptual hash, including RAW, HEIC and video via Quick Look.
- **Backup** — Mirror the local index to a PostgreSQL server and restore it from there.

## Install

Download the latest `.dmg` from [Releases](https://github.com/flying-turtles/OpenFileManager/releases), open it and drag **OpenFileManager** to Applications. The app is signed and notarized by Apple.

Requirements: macOS on Apple Silicon. Intel Macs need a build from source.

## Build from source

Prerequisites: [Node.js](https://nodejs.org/), [Rust](https://rustup.rs/), Xcode Command Line Tools.

```bash
npm install
npm run tauri dev     # run in development
npm run tauri build   # build .app and .dmg into src-tauri/target/release/bundle/
```

Run tests:

```bash
cd src-tauri && cargo test
```

### Signed release

`scripts/release.sh` builds, signs, notarizes and staples the DMG. It needs a *Developer ID Application* certificate in your keychain and a notarytool profile:

```bash
xcrun notarytool store-credentials ofm-notary
scripts/release.sh
```

Pass extra arguments to `tauri build`, e.g. `scripts/release.sh --target universal-apple-darwin` for an Apple Silicon + Intel build.

## How it works

- The index lives in a local SQLite database in the app's data directory.
- Paths are stored relative to each device's mount point. Registered drives get a small `.filemanagerid` marker file in their root, so they're recognised wherever they're mounted.
- The index uses a fast hash of each file's first 4 MB. Operations that delete or move data verify with a full-file hash and size check first.
- macOS only: uses `diskutil`, Quick Look (`qlmanage`), `mount_smbfs` and the Keychain.

## Tech stack

- **Frontend:** React, TypeScript, Vite
- **Backend:** Tauri 2 (Rust), SQLite (SQLx), BLAKE3

## License

[MIT](LICENSE)

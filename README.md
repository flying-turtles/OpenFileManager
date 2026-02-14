# OpenFileManager

A desktop app for tracking files across multiple storage devices, detecting duplicates by content (BLAKE3), and helping you see which files are safely backed up.

## What it does

- **Device tracking** — Detects mounted disks and volumes, shows capacity and usage. You can label each device as **hot** (primary/working) or **cold** (backup/archive).
- **File scanning** — Scans folders or whole drives, hashes files with BLAKE3, and stores where each unique file lives (which device and path).
- **Safety view** — A file is considered **safe** when it has at least one copy on a cold device and at least two copies in total. The app highlights **unsafe** files (single copy or no cold backup) so you can back them up.
- **File browser** — Browse files by device or list only unsafe files. Expand a file to see all copies and their safety status (hot/cold copies).

## How to use it

### Prerequisites

- [Node.js](https://nodejs.org/) (for the frontend)
- [Rust](https://rustup.rs/) (for the Tauri backend)

### Development

```bash
# Install frontend dependencies
npm install

# Run the app (builds frontend and opens the window)
npm run tauri dev
```

### Build for production

```bash
npm run tauri build
```

Outputs will be in `src-tauri/target/release/bundle/` (e.g. `.app` on macOS, `.dmg`, etc.).

### Using the app

1. **Dashboard** — Overview: unique file count, total copies, unsafe file count, devices, total size.
2. **Devices** — See all detected volumes. Set each as **hot**, **cold**, or **unknown**. Use **Refresh** to re-detect. Click **Scan** on a device to open the Scanner with that path.
3. **Scanner** — Choose a target path (type it, use **Browse**, or drag-and-drop a folder). Pick **Quick** (skip unchanged files, hash only files ≤ 2 GB) or **Full** (hash every file). Start the scan; you can cancel and run again later.
4. **Files** — Switch between **By Device** (pick a device and see its scanned files) and **Unsafe Only** (files that need a cold backup). Click a row to see copy count and all locations.

## Tech stack

- **Frontend:** React, TypeScript, Vite
- **Backend:** Tauri 2 (Rust), SQLite (via SQLx), BLAKE3 hashing

## Recommended IDE

- [VS Code](https://code.visualstudio.com/) with [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) and [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

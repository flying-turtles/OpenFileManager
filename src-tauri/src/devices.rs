use std::collections::HashSet;
use std::process::Command;

use sysinfo::Disks;

use crate::models::DetectedDisk;

const EXCLUDED_MOUNT_PREFIXES: &[&str] = &[
    "/System",
    "/Library",
    "/private",
    "/dev",
    "/home",
    "/cores",
];

const EXCLUDED_MOUNT_CONTAINS: &[&str] = &[
    "Preboot",
    "Recovery",
    "VM",
    "Update",
    "xarts",
    "iSCPreboot",
    "Hardware",
];

fn get_volume_uuid(mount_point: &str) -> Option<String> {
    let output = Command::new("diskutil")
        .args(["info", mount_point])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("Volume UUID:") || line.starts_with("Disk / Partition UUID:") {
            return line.split(':').nth(1).map(|s| s.trim().to_string());
        }
    }
    None
}

fn is_excluded(mount_point: &str) -> bool {
    for prefix in EXCLUDED_MOUNT_PREFIXES {
        if mount_point.starts_with(prefix) {
            return true;
        }
    }
    for pattern in EXCLUDED_MOUNT_CONTAINS {
        if mount_point.contains(pattern) {
            return true;
        }
    }
    false
}

pub fn detect_volumes() -> Vec<DetectedDisk> {
    let disks = Disks::new_with_refreshed_list();
    let mut result = Vec::new();
    let mut seen_mounts = HashSet::new();

    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy().to_string();
        if is_excluded(&mount) || !seen_mounts.insert(mount.clone()) {
            continue;
        }

        let uuid = match get_volume_uuid(&mount) {
            Some(u) => u,
            None => continue,
        };

        let label = disk.name().to_string_lossy().to_string();
        let label = if label.is_empty() {
            mount.rsplit('/').next().unwrap_or("Unknown").to_string()
        } else {
            label
        };

        result.push(DetectedDisk {
            id: uuid,
            label,
            mount_point: mount,
            total_bytes: disk.total_space() as i64,
            available_bytes: disk.available_space() as i64,
            is_removable: disk.is_removable(),
        });
    }

    result
}

/// Returns the device ID for a given path by finding which mount point contains it
pub fn device_for_path(devices: &[DetectedDisk], path: &str) -> Option<(String, String)> {
    let mut best_match: Option<(&DetectedDisk, usize)> = None;
    for dev in devices {
        if path.starts_with(&dev.mount_point) {
            let len = dev.mount_point.len();
            if best_match.is_none() || len > best_match.unwrap().1 {
                best_match = Some((dev, len));
            }
        }
    }
    best_match.map(|(dev, _)| (dev.id.clone(), dev.mount_point.clone()))
}

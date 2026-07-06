use std::path::Path;
use std::process::Command;

use crate::error::AppError;

pub fn generate_drive_id(protocol: &str, host: &str, share: &str) -> String {
    let input = format!("{}://{}/{}", protocol, host, share);
    let hash = blake3::hash(input.as_bytes());
    hash.to_hex()[..16].to_string()
}

pub fn keychain_store(drive_id: &str, username: &str, password: &str) -> Result<(), AppError> {
    let service = format!("ofm-{}", drive_id);
    let output = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-a", username,
            "-s", &service,
            "-w", password,
        ])
        .output()?;
    if !output.status.success() {
        return Err(AppError::General(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(())
}

pub fn keychain_load(drive_id: &str, username: &str) -> Result<String, AppError> {
    let service = format!("ofm-{}", drive_id);
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-a", username,
            "-s", &service,
            "-w",
        ])
        .output()?;
    if !output.status.success() {
        return Err(AppError::General(format!(
            "Failed to load keychain entry for {}",
            drive_id
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn keychain_delete(drive_id: &str, username: &str) -> Result<(), AppError> {
    let service = format!("ofm-{}", drive_id);
    let output = Command::new("security")
        .args([
            "delete-generic-password",
            "-a", username,
            "-s", &service,
        ])
        .output()?;
    if !output.status.success() {
        // Ignore errors if entry doesn't exist
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("could not be found") {
            return Err(AppError::General(stderr.to_string()));
        }
    }
    Ok(())
}

/// Mount points live in the user's home dir — /Volumes is root-only on
/// modern macOS, so creating a directory there fails with EACCES.
pub fn default_mount_point(label: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let safe: String = label
        .chars()
        .map(|c| if c == '/' || c == ':' { '_' } else { c })
        .collect();
    format!("{}/NetworkDrives/{}", home, safe)
}

pub fn mount_smb(
    host: &str,
    share: &str,
    username: &str,
    password: &str,
    mount_point: &str,
) -> Result<(), AppError> {
    let mount_path = Path::new(mount_point);
    if !mount_path.exists() {
        std::fs::create_dir_all(mount_path)?;
    }

    let url = if username.is_empty() {
        format!("//{}:{}", host, share)
    } else {
        format!("//{}:{}@{}/{}", username, password, host, share)
    };

    let output = Command::new("mount_smbfs")
        .args([&url, mount_point])
        .output()?;

    if !output.status.success() {
        let mut msg = String::from_utf8_lossy(&output.stderr).to_string();
        // Never echo credentials back in error messages
        if !password.is_empty() {
            msg = msg.replace(password, "***");
        }
        return Err(AppError::General(msg));
    }
    Ok(())
}

/// If this share is already mounted anywhere (e.g. mounted manually or via
/// Finder), return its current mount point — mount_smbfs refuses duplicate
/// mounts of the same share with "File exists".
pub fn find_existing_smb_mount(host: &str, share: &str) -> Option<String> {
    let output = Command::new("mount").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let needle = format!("@{}/{} on ", host, share).to_lowercase();
    for line in stdout.lines() {
        if !line.starts_with("//") {
            continue;
        }
        let lower = line.to_lowercase();
        if let Some(idx) = lower.find(&needle) {
            let rest = &line[idx + needle.len()..];
            let mp = rest.split(" (").next()?.trim();
            if !mp.is_empty() {
                return Some(mp.to_string());
            }
        }
    }
    None
}

pub fn mount_nfs(host: &str, export_path: &str, mount_point: &str) -> Result<(), AppError> {
    let mount_path = Path::new(mount_point);
    if !mount_path.exists() {
        std::fs::create_dir_all(mount_path)?;
    }

    let source = format!("{}:{}", host, export_path);
    let output = Command::new("mount")
        .args(["-t", "nfs", &source, mount_point])
        .output()?;

    if !output.status.success() {
        return Err(AppError::General(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(())
}

pub fn unmount_drive(mount_point: &str) -> Result<(), AppError> {
    let output = Command::new("umount")
        .arg(mount_point)
        .output()?;

    if !output.status.success() {
        return Err(AppError::General(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    // Remove empty mount dir
    let path = Path::new(mount_point);
    if path.exists() && path.read_dir().map(|mut d| d.next().is_none()).unwrap_or(false) {
        let _ = std::fs::remove_dir(path);
    }
    Ok(())
}

pub fn is_mountpoint(mount_point: &str) -> bool {
    let path = Path::new(mount_point);
    if !path.exists() {
        return false;
    }
    // Check via stat: if mount_point's device differs from parent's device, it's a mountpoint
    Command::new("mount")
        .output()
        .map(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.lines().any(|line| line.contains(&format!(" on {} ", mount_point)))
        })
        .unwrap_or(false)
}

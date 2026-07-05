use std::io::Read;
use std::path::Path;
use tokio::task::spawn_blocking;

use crate::error::AppError;

pub fn hash_file_partial_sync(path: &Path, max_bytes: u64) -> Result<String, AppError> {
    let mut hasher = blake3::Hasher::new();
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut remaining = max_bytes;
    loop {
        let to_read = (remaining as usize).min(buf.len());
        if to_read == 0 {
            break;
        }
        let n = file.read(&mut buf[..to_read])?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        remaining -= n as u64;
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub async fn hash_file_partial(path: &Path, max_bytes: u64) -> Result<String, AppError> {
    let path = path.to_path_buf();
    let hash = spawn_blocking(move || hash_file_partial_sync(&path, max_bytes))
        .await
        .map_err(|e| AppError::General(e.to_string()))??;
    Ok(hash)
}

pub fn hash_file_full_sync(path: &Path) -> Result<String, AppError> {
    let mut hasher = blake3::Hasher::new();
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

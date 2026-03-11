use std::path::Path;
use tokio::task::spawn_blocking;

use crate::error::AppError;

pub fn hash_file_sync(path: &Path) -> Result<String, AppError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_mmap(path)?;
    Ok(hasher.finalize().to_hex().to_string())
}

pub async fn hash_file(path: &Path) -> Result<String, AppError> {
    let path = path.to_path_buf();
    let hash = spawn_blocking(move || hash_file_sync(&path))
        .await
        .map_err(|e| AppError::General(e.to_string()))??;
    Ok(hash)
}

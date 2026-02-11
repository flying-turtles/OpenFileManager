use std::path::Path;
use tokio::task::spawn_blocking;

use crate::error::AppError;

pub async fn hash_file(path: &Path) -> Result<String, AppError> {
    let path = path.to_path_buf();
    let hash = spawn_blocking(move || -> Result<String, AppError> {
        let mut hasher = blake3::Hasher::new();
        hasher.update_mmap(&path)?;
        Ok(hasher.finalize().to_hex().to_string())
    })
    .await
    .map_err(|e| AppError::General(e.to_string()))??;

    Ok(hash)
}

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};

use crate::error::AppError;
use crate::models::*;

pub type DbPool = Pool<Sqlite>;

pub async fn init_pool(db_path: &Path) -> Result<DbPool, AppError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let url = format!("sqlite:{}?mode=rwc", db_path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await?;

    Ok(pool)
}

pub async fn run_migrations(pool: &DbPool) -> Result<(), AppError> {
    let sql = include_str!("../migrations/001_initial.sql");
    sqlx::raw_sql(sql).execute(pool).await?;
    let sql2 = include_str!("../migrations/002_projects.sql");
    sqlx::raw_sql(sql2).execute(pool).await?;
    Ok(())
}

// --- Device queries ---

pub async fn upsert_device(pool: &DbPool, disk: &DetectedDisk) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO storage_devices (id, label, mount_point, total_bytes, available_bytes, is_removable, first_seen, last_seen)
         VALUES (?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
           label = excluded.label,
           mount_point = excluded.mount_point,
           total_bytes = excluded.total_bytes,
           available_bytes = excluded.available_bytes,
           last_seen = datetime('now')"
    )
    .bind(&disk.id)
    .bind(&disk.label)
    .bind(&disk.mount_point)
    .bind(disk.total_bytes)
    .bind(disk.available_bytes)
    .bind(disk.is_removable)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_all_devices(pool: &DbPool) -> Result<Vec<StorageDevice>, AppError> {
    let devices = sqlx::query_as::<_, StorageDevice>("SELECT * FROM storage_devices ORDER BY last_seen DESC")
        .fetch_all(pool)
        .await?;
    Ok(devices)
}

pub async fn set_device_type(pool: &DbPool, device_id: &str, device_type: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE storage_devices SET device_type = ? WHERE id = ?")
        .bind(device_type)
        .bind(device_id)
        .execute(pool)
        .await?;
    Ok(())
}

// --- File queries ---

pub async fn upsert_file(pool: &DbPool, hash: &str, size: i64, name: &str, ext: &str) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO files (blake3_hash, file_size, representative_name, extension)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(blake3_hash) DO NOTHING"
    )
    .bind(hash)
    .bind(size)
    .bind(name)
    .bind(ext)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_location(
    pool: &DbPool,
    hash: &str,
    device_id: &str,
    file_path: &str,
    file_name: &str,
    file_size: i64,
    modified_at: Option<&str>,
    scan_mode: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO file_locations (blake3_hash, device_id, file_path, file_name, file_size, modified_at, last_verified, scan_mode)
         VALUES (?, ?, ?, ?, ?, ?, datetime('now'), ?)
         ON CONFLICT(device_id, file_path) DO UPDATE SET
           blake3_hash = excluded.blake3_hash,
           file_name = excluded.file_name,
           file_size = excluded.file_size,
           modified_at = excluded.modified_at,
           last_verified = datetime('now'),
           scan_mode = excluded.scan_mode"
    )
    .bind(hash)
    .bind(device_id)
    .bind(file_path)
    .bind(file_name)
    .bind(file_size)
    .bind(modified_at)
    .bind(scan_mode)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_existing_location(
    pool: &DbPool,
    device_id: &str,
    file_path: &str,
) -> Result<Option<FileLocation>, AppError> {
    let loc = sqlx::query_as::<_, FileLocation>(
        "SELECT * FROM file_locations WHERE device_id = ? AND file_path = ?"
    )
    .bind(device_id)
    .bind(file_path)
    .fetch_optional(pool)
    .await?;
    Ok(loc)
}

pub async fn get_files_on_device(pool: &DbPool, device_id: &str) -> Result<Vec<FileLocation>, AppError> {
    let locs = sqlx::query_as::<_, FileLocation>(
        "SELECT * FROM file_locations WHERE device_id = ? ORDER BY file_path"
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?;
    Ok(locs)
}

pub async fn get_file_locations(pool: &DbPool, hash: &str) -> Result<Vec<FileLocation>, AppError> {
    let locs = sqlx::query_as::<_, FileLocation>(
        "SELECT * FROM file_locations WHERE blake3_hash = ?"
    )
    .bind(hash)
    .fetch_all(pool)
    .await?;
    Ok(locs)
}

pub async fn get_file_safety(pool: &DbPool, hash: &str) -> Result<Option<FileSafety>, AppError> {
    let row = sqlx::query_as::<_, (String, i64, String, i64, i64, i64)>(
        "SELECT f.blake3_hash, f.file_size, f.representative_name,
                COUNT(fl.id) as total_copies,
                COALESCE(SUM(CASE WHEN d.device_type = 'hot' THEN 1 ELSE 0 END), 0) as hot_copies,
                COALESCE(SUM(CASE WHEN d.device_type = 'cold' THEN 1 ELSE 0 END), 0) as cold_copies
         FROM files f
         JOIN file_locations fl ON f.blake3_hash = fl.blake3_hash
         JOIN storage_devices d ON fl.device_id = d.id
         WHERE f.blake3_hash = ?
         GROUP BY f.blake3_hash"
    )
    .bind(hash)
    .fetch_optional(pool)
    .await?;

    match row {
        Some((blake3_hash, file_size, representative_name, total_copies, hot_copies, cold_copies)) => {
            let locations = get_file_locations(pool, &blake3_hash).await?;
            let is_safe = cold_copies >= 1 && total_copies >= 2;
            Ok(Some(FileSafety {
                blake3_hash,
                file_size,
                representative_name,
                total_copies,
                hot_copies,
                cold_copies,
                is_safe,
                locations,
            }))
        }
        None => Ok(None),
    }
}

pub async fn get_unsafe_files(pool: &DbPool) -> Result<Vec<FileSafety>, AppError> {
    let rows = sqlx::query_as::<_, (String, i64, String, i64, i64, i64)>(
        "SELECT f.blake3_hash, f.file_size, f.representative_name,
                COUNT(fl.id) as total_copies,
                COALESCE(SUM(CASE WHEN d.device_type = 'hot' THEN 1 ELSE 0 END), 0) as hot_copies,
                COALESCE(SUM(CASE WHEN d.device_type = 'cold' THEN 1 ELSE 0 END), 0) as cold_copies
         FROM files f
         JOIN file_locations fl ON f.blake3_hash = fl.blake3_hash
         JOIN storage_devices d ON fl.device_id = d.id
         GROUP BY f.blake3_hash
         HAVING cold_copies < 1 OR total_copies < 2
         ORDER BY f.file_size DESC"
    )
    .fetch_all(pool)
    .await?;

    let mut results = Vec::new();
    for (blake3_hash, file_size, representative_name, total_copies, hot_copies, cold_copies) in rows {
        let locations = get_file_locations(pool, &blake3_hash).await?;
        results.push(FileSafety {
            blake3_hash,
            file_size,
            representative_name,
            total_copies,
            hot_copies,
            cold_copies,
            is_safe: false,
            locations,
        });
    }
    Ok(results)
}

pub async fn get_waste_candidates(pool: &DbPool, threshold: i64) -> Result<Vec<WasteCandidate>, AppError> {
    let rows = sqlx::query_as::<_, WasteCandidate>(
        "SELECT f.blake3_hash, f.file_size, f.representative_name,
                COUNT(fl.id) as total_copies,
                f.file_size * (COUNT(fl.id) - 1) as wasted_bytes
         FROM files f
         JOIN file_locations fl ON f.blake3_hash = fl.blake3_hash
         GROUP BY f.blake3_hash
         HAVING total_copies > ?
         ORDER BY wasted_bytes DESC"
    )
    .bind(threshold)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn remove_stale_locations(
    pool: &DbPool,
    device_id: &str,
    path_prefix: &str,
    seen_paths: &[String],
) -> Result<u64, AppError> {
    // Delete locations under the scanned prefix that weren't seen
    // Use a prefix match with LIKE (escape % and _ in prefix)
    let prefix_pattern = format!(
        "{}%",
        path_prefix.replace('%', "\\%").replace('_', "\\_")
    );

    if seen_paths.is_empty() {
        // Nothing seen = everything under prefix is gone
        let res = sqlx::query(
            "DELETE FROM file_locations WHERE device_id = ? AND file_path LIKE ? ESCAPE '\\'"
        )
        .bind(device_id)
        .bind(&prefix_pattern)
        .execute(pool)
        .await?;
        return Ok(res.rows_affected());
    }

    // Build a temp table approach: insert seen paths, delete those not in it
    // For simplicity, batch delete with NOT IN (chunked to avoid SQLite limits)
    let mut total_deleted: u64 = 0;
    // Get all existing locations under prefix
    let existing = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, file_path FROM file_locations WHERE device_id = ? AND file_path LIKE ? ESCAPE '\\'"
    )
    .bind(device_id)
    .bind(&prefix_pattern)
    .fetch_all(pool)
    .await?;

    let seen_set: std::collections::HashSet<&str> = seen_paths.iter().map(|s| s.as_str()).collect();
    let stale_ids: Vec<i64> = existing
        .iter()
        .filter(|(_, path)| !seen_set.contains(path.as_str()))
        .map(|(id, _)| *id)
        .collect();

    for chunk in stale_ids.chunks(500) {
        let placeholders: String = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("DELETE FROM file_locations WHERE id IN ({})", placeholders);
        let mut query = sqlx::query(&sql);
        for id in chunk {
            query = query.bind(id);
        }
        let res = query.execute(pool).await?;
        total_deleted += res.rows_affected();
    }

    Ok(total_deleted)
}

pub async fn cleanup_orphaned_files(pool: &DbPool) -> Result<u64, AppError> {
    let res = sqlx::query(
        "DELETE FROM files WHERE blake3_hash NOT IN (SELECT DISTINCT blake3_hash FROM file_locations)"
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn get_locations_for_hashes(
    pool: &DbPool,
    hashes: &[String],
) -> Result<HashMap<String, Vec<FileLocation>>, AppError> {
    let mut result: HashMap<String, Vec<FileLocation>> = HashMap::new();
    for chunk in hashes.chunks(500) {
        let placeholders: String = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT * FROM file_locations WHERE blake3_hash IN ({})",
            placeholders
        );
        let mut query = sqlx::query_as::<_, FileLocation>(&sql);
        for hash in chunk {
            query = query.bind(hash);
        }
        let locs = query.fetch_all(pool).await?;
        for loc in locs {
            result.entry(loc.blake3_hash.clone()).or_default().push(loc);
        }
    }
    Ok(result)
}

// --- Project queries ---

pub async fn create_project(
    pool: &DbPool,
    title: &str,
    description: &str,
    start_date: &str,
    end_date: &str,
) -> Result<Project, AppError> {
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO projects (title, description, start_date, end_date) VALUES (?, ?, ?, ?) RETURNING id"
    )
    .bind(title)
    .bind(description)
    .bind(start_date)
    .bind(end_date)
    .fetch_one(pool)
    .await?;

    let project = sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(project)
}

pub async fn get_all_projects(pool: &DbPool) -> Result<Vec<Project>, AppError> {
    let projects = sqlx::query_as::<_, Project>("SELECT * FROM projects ORDER BY start_date DESC")
        .fetch_all(pool)
        .await?;
    Ok(projects)
}

pub async fn get_project(pool: &DbPool, id: i64) -> Result<Project, AppError> {
    let project = sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(project)
}

pub async fn update_project(
    pool: &DbPool,
    id: i64,
    title: &str,
    description: &str,
    start_date: &str,
    end_date: &str,
) -> Result<Project, AppError> {
    sqlx::query("UPDATE projects SET title = ?, description = ?, start_date = ?, end_date = ? WHERE id = ?")
        .bind(title)
        .bind(description)
        .bind(start_date)
        .bind(end_date)
        .bind(id)
        .execute(pool)
        .await?;
    get_project(pool, id).await
}

pub async fn delete_project(pool: &DbPool, id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_project_files(
    pool: &DbPool,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<FileSafety>, AppError> {
    // Find files whose MIN(modified_at) falls in [start_date, end_date + 1 day)
    let rows = sqlx::query_as::<_, (String, i64, String, i64, i64, i64)>(
        "SELECT f.blake3_hash, f.file_size, f.representative_name,
                COUNT(fl.id) as total_copies,
                COALESCE(SUM(CASE WHEN d.device_type = 'hot' THEN 1 ELSE 0 END), 0) as hot_copies,
                COALESCE(SUM(CASE WHEN d.device_type = 'cold' THEN 1 ELSE 0 END), 0) as cold_copies
         FROM files f
         JOIN file_locations fl ON f.blake3_hash = fl.blake3_hash
         JOIN storage_devices d ON fl.device_id = d.id
         WHERE f.blake3_hash IN (
             SELECT fl2.blake3_hash
             FROM file_locations fl2
             GROUP BY fl2.blake3_hash
             HAVING MIN(fl2.modified_at) >= ? AND MIN(fl2.modified_at) < date(?, '+1 day')
         )
         GROUP BY f.blake3_hash
         ORDER BY f.file_size DESC"
    )
    .bind(start_date)
    .bind(end_date)
    .fetch_all(pool)
    .await?;

    let mut results = Vec::new();
    for (blake3_hash, file_size, representative_name, total_copies, hot_copies, cold_copies) in rows {
        let locations = get_file_locations(pool, &blake3_hash).await?;
        let is_safe = cold_copies >= 1 && total_copies >= 2;
        results.push(FileSafety {
            blake3_hash,
            file_size,
            representative_name,
            total_copies,
            hot_copies,
            cold_copies,
            is_safe,
            locations,
        });
    }
    Ok(results)
}

pub async fn get_project_stats(
    pool: &DbPool,
    start_date: &str,
    end_date: &str,
) -> Result<ProjectStats, AppError> {
    let base_cte = "WITH project_files AS (
        SELECT fl2.blake3_hash
        FROM file_locations fl2
        GROUP BY fl2.blake3_hash
        HAVING MIN(fl2.modified_at) >= ? AND MIN(fl2.modified_at) < date(?, '+1 day')
    )";

    let counts: (i64, i64) = sqlx::query_as(&format!(
        "{} SELECT COUNT(*), COALESCE(SUM(f.file_size), 0)
         FROM files f
         WHERE f.blake3_hash IN (SELECT blake3_hash FROM project_files)",
        base_cte
    ))
    .bind(start_date)
    .bind(end_date)
    .fetch_one(pool)
    .await?;

    let backed_up: (i64,) = sqlx::query_as(&format!(
        "{} SELECT COUNT(*) FROM (
            SELECT f.blake3_hash
            FROM files f
            JOIN file_locations fl ON f.blake3_hash = fl.blake3_hash
            JOIN storage_devices d ON fl.device_id = d.id
            WHERE f.blake3_hash IN (SELECT blake3_hash FROM project_files)
            GROUP BY f.blake3_hash
            HAVING COUNT(fl.id) >= 2 AND SUM(CASE WHEN d.device_type = 'cold' THEN 1 ELSE 0 END) >= 1
        )",
        base_cte
    ))
    .bind(start_date)
    .bind(end_date)
    .fetch_one(pool)
    .await?;

    let ext_rows = sqlx::query_as::<_, (String, i64)>(&format!(
        "{} SELECT COALESCE(f.extension, ''), COUNT(*) as cnt
         FROM files f
         WHERE f.blake3_hash IN (SELECT blake3_hash FROM project_files)
         GROUP BY f.extension
         ORDER BY cnt DESC",
        base_cte
    ))
    .bind(start_date)
    .bind(end_date)
    .fetch_all(pool)
    .await?;

    let extensions = ext_rows
        .into_iter()
        .map(|(extension, count)| ExtensionCount { extension, count })
        .collect();

    let backed_up_pct = if counts.0 > 0 {
        (backed_up.0 as f64 / counts.0 as f64) * 100.0
    } else {
        0.0
    };

    Ok(ProjectStats {
        total_files: counts.0,
        total_size_bytes: counts.1,
        backed_up_pct,
        extensions,
    })
}

pub async fn get_dashboard_stats(pool: &DbPool) -> Result<DashboardStats, AppError> {
    let total_files: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM files")
        .fetch_one(pool)
        .await?;
    let total_locations: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM file_locations")
        .fetch_one(pool)
        .await?;
    let total_devices: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM storage_devices")
        .fetch_one(pool)
        .await?;
    let total_size: (i64,) = sqlx::query_as("SELECT COALESCE(SUM(file_size), 0) FROM files")
        .fetch_one(pool)
        .await?;

    // Count unsafe files: those without cold_copies >= 1 AND total_copies >= 2
    let unsafe_files: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM (
            SELECT f.blake3_hash,
                   COUNT(fl.id) as total_copies,
                   COALESCE(SUM(CASE WHEN d.device_type = 'cold' THEN 1 ELSE 0 END), 0) as cold_copies
            FROM files f
            JOIN file_locations fl ON f.blake3_hash = fl.blake3_hash
            JOIN storage_devices d ON fl.device_id = d.id
            GROUP BY f.blake3_hash
            HAVING cold_copies < 1 OR total_copies < 2
        )"
    )
    .fetch_one(pool)
    .await?;

    Ok(DashboardStats {
        total_files: total_files.0,
        total_locations: total_locations.0,
        unsafe_files: unsafe_files.0,
        total_devices: total_devices.0,
        total_size_bytes: total_size.0,
    })
}

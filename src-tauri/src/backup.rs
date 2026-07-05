use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Row};
use tauri::ipc::Channel;

use crate::db::DbPool;
use crate::error::AppError;
use crate::models::BackupEvent;

/// Pseudo drive id used to namespace the Postgres password in the Keychain,
/// reusing the same helpers as network drive credentials.
pub const KEYCHAIN_ID: &str = "pg-backup";

const INSERT_CHUNK: usize = 500;

pub async fn connect(
    host: &str,
    port: u16,
    database: &str,
    username: &str,
    password: &str,
) -> Result<PgPool, AppError> {
    let opts = PgConnectOptions::new()
        .host(host)
        .port(port)
        .database(database)
        .username(username)
        .password(password);
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect_with(opts)
        .await
        .map_err(|e| AppError::General(format!("Could not connect to PostgreSQL: {}", e)))?;
    Ok(pool)
}

#[derive(Clone, Copy)]
enum ColType {
    Text,
    OptText,
    Int,
}

struct TableSpec {
    name: &'static str,
    /// SELECT over the local SQLite database
    select: &'static str,
    /// CREATE TABLE for the Postgres mirror
    create: &'static str,
    columns: &'static [(&'static str, ColType)],
}

/// Durable tables only — pending_scans and dir_cache are transient caches.
const TABLES: &[TableSpec] = &[
    TableSpec {
        name: "storage_devices",
        select: "SELECT id, label, mount_point, device_type, total_bytes, available_bytes, is_removable, first_seen, last_seen, drive_speed FROM storage_devices",
        create: "CREATE TABLE storage_devices (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            mount_point TEXT NOT NULL,
            device_type TEXT NOT NULL,
            total_bytes BIGINT NOT NULL,
            available_bytes BIGINT NOT NULL,
            is_removable BIGINT NOT NULL,
            first_seen TEXT NOT NULL,
            last_seen TEXT NOT NULL,
            drive_speed TEXT NOT NULL
        )",
        columns: &[("id", ColType::Text), ("label", ColType::Text), ("mount_point", ColType::Text), ("device_type", ColType::Text), ("total_bytes", ColType::Int), ("available_bytes", ColType::Int), ("is_removable", ColType::Int), ("first_seen", ColType::Text), ("last_seen", ColType::Text), ("drive_speed", ColType::Text)],
    },
    TableSpec {
        name: "files",
        select: "SELECT blake3_hash, file_size, representative_name, extension, created_at FROM files",
        create: "CREATE TABLE files (
            blake3_hash TEXT PRIMARY KEY,
            file_size BIGINT NOT NULL,
            representative_name TEXT NOT NULL,
            extension TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
        columns: &[("blake3_hash", ColType::Text), ("file_size", ColType::Int), ("representative_name", ColType::Text), ("extension", ColType::Text), ("created_at", ColType::Text)],
    },
    TableSpec {
        name: "file_locations",
        select: "SELECT id, blake3_hash, device_id, file_path, file_name, file_size, modified_at, last_verified, scan_mode FROM file_locations",
        create: "CREATE TABLE file_locations (
            id BIGINT PRIMARY KEY,
            blake3_hash TEXT NOT NULL,
            device_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            file_name TEXT NOT NULL,
            file_size BIGINT NOT NULL,
            modified_at TEXT,
            last_verified TEXT NOT NULL,
            scan_mode TEXT NOT NULL
        )",
        columns: &[("id", ColType::Int), ("blake3_hash", ColType::Text), ("device_id", ColType::Text), ("file_path", ColType::Text), ("file_name", ColType::Text), ("file_size", ColType::Int), ("modified_at", ColType::OptText), ("last_verified", ColType::Text), ("scan_mode", ColType::Text)],
    },
    TableSpec {
        name: "projects",
        select: "SELECT id, title, description, start_date, end_date, created_at FROM projects",
        create: "CREATE TABLE projects (
            id BIGINT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            start_date TEXT NOT NULL,
            end_date TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
        columns: &[("id", ColType::Int), ("title", ColType::Text), ("description", ColType::Text), ("start_date", ColType::Text), ("end_date", ColType::Text), ("created_at", ColType::Text)],
    },
    TableSpec {
        name: "network_drives",
        select: "SELECT id, label, protocol, host, share_path, username, mount_point, device_type, created_at FROM network_drives",
        create: "CREATE TABLE network_drives (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            protocol TEXT NOT NULL,
            host TEXT NOT NULL,
            share_path TEXT NOT NULL,
            username TEXT NOT NULL,
            mount_point TEXT NOT NULL,
            device_type TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
        columns: &[("id", ColType::Text), ("label", ColType::Text), ("protocol", ColType::Text), ("host", ColType::Text), ("share_path", ColType::Text), ("username", ColType::Text), ("mount_point", ColType::Text), ("device_type", ColType::Text), ("created_at", ColType::Text)],
    },
];

/// One SQLite row converted to Postgres bind values.
enum Val {
    Text(String),
    OptText(Option<String>),
    Int(i64),
}

fn read_row(
    row: &sqlx::sqlite::SqliteRow,
    columns: &[(&str, ColType)],
) -> Result<Vec<Val>, AppError> {
    let mut vals = Vec::with_capacity(columns.len());
    for (i, (name, col_type)) in columns.iter().enumerate() {
        let val = match col_type {
            ColType::Int => Val::Int(
                row.try_get::<i64, _>(i)
                    .map_err(|e| AppError::General(format!("column {}: {}", name, e)))?,
            ),
            ColType::Text => Val::Text(
                row.try_get::<String, _>(i)
                    .map_err(|e| AppError::General(format!("column {}: {}", name, e)))?,
            ),
            ColType::OptText => Val::OptText(
                row.try_get::<Option<String>, _>(i)
                    .map_err(|e| AppError::General(format!("column {}: {}", name, e)))?,
            ),
        };
        vals.push(val);
    }
    Ok(vals)
}

/// Mirror the local SQLite database into Postgres. The local database is the
/// source of truth: each table on the server is dropped and rebuilt inside a
/// single transaction, so a failed backup leaves the previous backup intact.
pub async fn run_backup(
    sqlite: &DbPool,
    pg: &PgPool,
    channel: &Channel<BackupEvent>,
) -> Result<u64, AppError> {
    let _ = channel.send(BackupEvent::Started {
        total_tables: TABLES.len() as u64,
    });

    let mut tx = pg
        .begin()
        .await
        .map_err(|e| AppError::General(format!("PostgreSQL error: {}", e)))?;

    let mut total_rows: u64 = 0;

    for spec in TABLES {
        let rows = sqlx::query(spec.select).fetch_all(sqlite).await?;
        let table_total = rows.len() as u64;

        sqlx::query(&format!("DROP TABLE IF EXISTS {}", spec.name))
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::General(format!("PostgreSQL error: {}", e)))?;
        sqlx::query(spec.create)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::General(format!("PostgreSQL error: {}", e)))?;

        let mut copied: u64 = 0;
        for chunk in rows.chunks(INSERT_CHUNK) {
            let col_names: Vec<&str> = spec.columns.iter().map(|(n, _)| *n).collect();
            let mut sql = format!(
                "INSERT INTO {} ({}) VALUES ",
                spec.name,
                col_names.join(", ")
            );
            let mut param = 1;
            for (r, _) in chunk.iter().enumerate() {
                if r > 0 {
                    sql.push(',');
                }
                sql.push('(');
                for c in 0..spec.columns.len() {
                    if c > 0 {
                        sql.push(',');
                    }
                    sql.push_str(&format!("${}", param));
                    param += 1;
                }
                sql.push(')');
            }

            let mut query = sqlx::query(&sql);
            for row in chunk {
                for val in read_row(row, spec.columns)? {
                    query = match val {
                        Val::Text(s) => query.bind(s),
                        Val::OptText(o) => query.bind(o),
                        Val::Int(n) => query.bind(n),
                    };
                }
            }
            query
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::General(format!("PostgreSQL error: {}", e)))?;

            copied += chunk.len() as u64;
            let _ = channel.send(BackupEvent::TableProgress {
                table: spec.name.to_string(),
                rows_copied: copied,
                total_rows: table_total,
            });
        }

        total_rows += table_total;
        let _ = channel.send(BackupEvent::TableDone {
            table: spec.name.to_string(),
            rows: table_total,
        });
    }

    tx.commit()
        .await
        .map_err(|e| AppError::General(format!("PostgreSQL error: {}", e)))?;

    Ok(total_rows)
}

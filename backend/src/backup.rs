//! 备份与恢复：把库里所有内容连同媒体文件打成一个 zip，也能再吃回去。
//!
//! # 包的两副面孔
//!
//! 媒体库动辄几十上百 G，每周全量重打一遍既慢又费盘。所以包分成两种：
//!
//! - **base**：完整快照，数据全量 + 全部媒体。第一次打，之后每隔一阵重打一个。
//! - **delta**：数据快照仍是全量（几万行也就几十 MB，不值得为它做增量），
//!   但**媒体只装自上次以来新增的**——媒体文件一旦落盘就不再改动，
//!   所以「created_at 晚于上次备份」就是新增的全部，不需要比对内容。
//!
//! 恢复时把 base 与之后的 delta 按文件名里的时间戳排序，从旧到新依次重放，
//! 就能还原到最后一个包落地的时刻。所以**同一链上的包不能单独删**。
//!
//! # 改动与删除
//!
//! 快照是全量的，所以「改了什么」天然能看出来——合并时按 `updated_at` 比，
//! 谁新听谁的。但**删除**在快照里消失了，光看快照分不清「被删了」和
//! 「从没存在过」，于是删除时往 `deletions` 表落一笔，增量包带上它，
//! 恢复时照着重放。打完新 base 后留痕清空——完整快照本身就是真值。
//!
//! # 包内结构
//!
//! ```text
//! manifest.json   格式版本、包类型（base/delta）、基准时刻与统计
//! data.json       五张表的内容 + 删除留痕
//! files/…         本包携带的媒体文件，路径与数据目录一一对应
//! ```
//!
//! 打包全程流式：一个文件一个文件往 zip 里写，不把整包读进内存。媒体本来就是
//! 压缩格式，再压一遍只是白费 CPU，所以它们走 Stored，只有两个 json 走 Deflated。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use axum::extract::{Multipart, Path as UrlPath, State};
use axum::http::header;
use axum::response::Response;
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{AppError, AppResult};
use crate::media::Storage;
use crate::state::AppState;

const FORMAT_VERSION: u32 = 2;
const MANIFEST_NAME: &str = "manifest.json";
const DATA_NAME: &str = "data.json";
const FILES_PREFIX: &str = "files/";
const FILE_PREFIX: &str = "efflux-";

const KIND_BASE: &str = "base";
const KIND_DELTA: &str = "delta";

/// 链上攒够这么多增量，就重打一个基准：恢复时不必攒一长串包
const MAX_CHAIN: usize = 14;
/// 距上一个基准超过这么多天，也重打一个
const MAX_BASE_AGE_DAYS: i64 = 30;

/// 自动备份的间隔（天）存在这里，设置页改的就是它
const INTERVAL_KEY: &str = "backup.interval_days";
/// 上次打包的状态（上一个包的时间、当前链的基准）也存在这里
const STATE_KEY: &str = "backup.state";
/// 备份目录也在这里存着。空 = 用数据目录下的 backups/
const DIR_KEY: &str = "backup.dir";

const DEFAULT_INTERVAL_DAYS: i64 = 7;
/// 兜底：哪怕配置坏了也别超过这个
const MAX_INTERVAL_DAYS: i64 = 90;

/// 导入包的大小上限：32G。一份 base 本来就大
const MAX_IMPORT_BYTES: u64 = 32 * 1024 * 1024 * 1024;

// ---------------------------------------------------------------- 包内结构

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format_version: u32,
    /// "base" 或 "delta"。老包（v1）没有这个字段，一律当基准——
    /// 那时候的包本来就是全量
    #[serde(default = "default_kind")]
    pub kind: String,
    pub exported_at: DateTime<Utc>,
    /// delta 的基准时刻：本包只携带此刻之后新增的文件
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
    pub counts: Counts,
}

fn default_kind() -> String {
    KIND_BASE.to_string()
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub moments: usize,
    pub media: usize,
    pub tags: usize,
    pub files: usize,
    pub deletions: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupData {
    #[serde(default)]
    pub moments: Vec<MomentBackup>,
    #[serde(default)]
    pub media: Vec<MediaBackup>,
    #[serde(default)]
    pub tags: Vec<TagBackup>,
    #[serde(default)]
    pub moment_tags: Vec<MomentTagBackup>,
    #[serde(default)]
    pub media_tags: Vec<MediaTagBackup>,
    /// 上一次打包之后删掉的东西。基准包里没有这个（快照本身就是真值）
    #[serde(default)]
    pub deletions: Vec<DeletionBackup>,
}

/// 各表的行。老备份里没有的新字段一律 `#[serde(default)]`，
/// 这样以前导出的包今天还能吃进来。
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MomentBackup {
    pub id: Uuid,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub content: String,
    pub happened_at: DateTime<Utc>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub location_name: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MediaBackup {
    pub id: Uuid,
    #[serde(default)]
    pub moment_id: Option<Uuid>,
    pub kind: String,
    #[serde(default)]
    pub role: String,
    pub file_path: String,
    #[serde(default)]
    pub thumb_path: Option<String>,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub size_bytes: i64,
    #[serde(default)]
    pub width: Option<i32>,
    #[serde(default)]
    pub height: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<i32>,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub tag_status: String,
    #[serde(default)]
    pub tagged_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TagBackup {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MomentTagBackup {
    pub moment_id: Uuid,
    pub tag_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MediaTagBackup {
    pub media_id: Uuid,
    pub tag_key: String,
    #[serde(default)]
    pub tag_name: String,
    #[serde(default)]
    pub tag_group: String,
    #[serde(default)]
    pub score: f32,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DeletionBackup {
    pub id: Uuid,
    pub table_name: String,
    pub deleted_at: DateTime<Utc>,
}

// ---------------------------------------------------------------- 对外结果

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    pub name: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    /// "base" 或 "delta"，前端据此标出这一包的分量
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    /// 新写进去的
    pub moments_added: u64,
    /// 库里已经一样新、跳过的
    pub moments_skipped: u64,
    pub media_added: u64,
    pub media_skipped: u64,
    pub tags_added: u64,
    pub files_written: u64,
    pub files_skipped: u64,
    /// 按留痕重放掉的删除
    pub deletions_applied: u64,
    /// 没吃下的包（坏包、版本不认），一个包一条说明
    pub failed: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettings {
    /// 自动备份间隔（天）。0 表示关掉，只手动导出
    pub interval_days: i64,
    /// 备份目录（绝对路径）
    pub dir: String,
    /// 默认位置，页面上给个「用默认」的参照
    pub default_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettingsInput {
    pub interval_days: i64,
    /// 留空表示回到默认位置
    #[serde(default)]
    pub dir: Option<String>,
}

/// 上次打包的状态。存进 app_settings，重启后接着走同一条链
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupState {
    last_at: Option<DateTime<Utc>>,
    base_name: Option<String>,
    base_at: Option<DateTime<Utc>>,
    /// 当前链上已经攒了几个增量
    deltas: usize,
}

// ---------------------------------------------------------------- 备份目录与配置

/// 备份的默认位置：数据目录下的 backups/
pub fn default_dir(storage: &Storage) -> PathBuf {
    storage.root().join("backups")
}

/// 当前的备份目录：设置里指定过就用它，否则回到默认位置
pub async fn backup_dir_for(pool: &PgPool, storage: &Storage) -> AppResult<PathBuf> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM app_settings WHERE key = $1")
        .bind(DIR_KEY)
        .fetch_optional(pool)
        .await?;

    Ok(row
        .map(|(value,)| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_dir(storage)))
}

async fn save_dir(pool: &PgPool, dir: &Path) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(DIR_KEY)
    .bind(dir.to_string_lossy().to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// 把设置里的目录收拾干净：留空就回默认，必须是绝对路径，能建、还得能写。
///
/// 顺手写一个探针文件——「建得出来」不等于「写得进去」，
/// 权限、只读挂载、磁盘满，都常常卡在这一步。
async fn prepare_dir(raw: &str, fallback: &Path) -> AppResult<PathBuf> {
    let trimmed = raw.trim();
    let path = if trimmed.is_empty() {
        fallback.to_path_buf()
    } else {
        let candidate = PathBuf::from(trimmed);
        if !candidate.is_absolute() {
            return Err(AppError::validation(
                "请填一个绝对路径，比如 /mnt/backup/efflux",
            ));
        }
        candidate
    };

    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|e| AppError::validation(format!("这个目录建不出来：{e}")))?;

    let probe = path.join(".efflux-write-test");
    tokio::fs::write(&probe, b"ok")
        .await
        .map_err(|e| AppError::validation(format!("这个目录写不进去：{e}")))?;
    let _ = tokio::fs::remove_file(&probe).await;

    Ok(path)
}

/// 换目录时把已有的包搬过去。
///
/// 不搬的话恢复链在新地方就断了——一堆 delta 孤零零地摆着，前面那个 base
/// 还留在老地方。同盘改名是瞬时的，只有跨盘才会真复制一份。
async fn migrate_archives(from: &Path, to: &Path) -> AppResult<(u64, u64)> {
    if from == to {
        return Ok((0, 0));
    }

    let mut entries = match tokio::fs::read_dir(from).await {
        Ok(entries) => entries,
        // 旧目录本来就没有，没什么可搬的
        Err(_) => return Ok((0, 0)),
    };

    let mut moved = 0u64;
    let mut failed = 0u64;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(FILE_PREFIX) || !name.ends_with(".zip") {
            continue;
        }

        let target = to.join(&name);
        if target.exists() {
            // 新目录已经有同名包了，不覆盖
            continue;
        }

        if tokio::fs::rename(entry.path(), &target).await.is_ok() {
            moved += 1;
            continue;
        }

        match tokio::fs::copy(entry.path(), &target).await {
            Ok(_) => {
                let _ = tokio::fs::remove_file(entry.path()).await;
                moved += 1;
            }
            Err(err) => {
                tracing::warn!(name = %name, error = %err, "这个包没能搬过去");
                failed += 1;
            }
        }
    }

    Ok((moved, failed))
}

/// 自动备份间隔（天）。0 表示关掉。
pub async fn interval_days(pool: &PgPool) -> AppResult<i64> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM app_settings WHERE key = $1")
        .bind(INTERVAL_KEY)
        .fetch_optional(pool)
        .await?;

    Ok(row
        .and_then(|(value,)| value.trim().parse::<i64>().ok())
        .filter(|days| *days >= 0 && *days <= MAX_INTERVAL_DAYS)
        .unwrap_or(DEFAULT_INTERVAL_DAYS))
}

pub async fn set_interval_days(pool: &PgPool, days: i64) -> AppResult<i64> {
    let days = days.clamp(0, MAX_INTERVAL_DAYS);
    sqlx::query(
        "INSERT INTO app_settings (key, value, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(INTERVAL_KEY)
    .bind(days.to_string())
    .execute(pool)
    .await?;
    Ok(days)
}

async fn load_state(pool: &PgPool) -> AppResult<BackupState> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM app_settings WHERE key = $1")
        .bind(STATE_KEY)
        .fetch_optional(pool)
        .await?;
    Ok(row
        .and_then(|(value,)| serde_json::from_str(&value).ok())
        .unwrap_or_default())
}

async fn save_state(pool: &PgPool, state: &BackupState) -> AppResult<()> {
    let value = serde_json::to_string(state).map_err(|e| AppError::Internal(e.into()))?;
    sqlx::query(
        "INSERT INTO app_settings (key, value, updated_at) VALUES ($1, $2, now()) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(STATE_KEY)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------- 打包

/// 打一份备份。第一次或链条到期时是基准包，否则是增量包。
pub async fn create(pool: &PgPool, storage: &Storage) -> AppResult<BackupFile> {
    let state = load_state(pool).await?;
    let now = Utc::now();

    // 该重打基准了吗：没有基准、链太长、或者基准太旧
    let base_due = state.base_at.is_none()
        || state.deltas >= MAX_CHAIN
        || state
            .base_at
            .map_or(true, |at| (now - at).num_days() >= MAX_BASE_AGE_DAYS);
    let kind = if base_due { KIND_BASE } else { KIND_DELTA };

    // 增量只装自上次以来新增的媒体；基准装全部
    let since = if base_due { None } else { state.last_at };

    let (data, files) = collect(pool, since).await?;

    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        kind: kind.to_string(),
        exported_at: now,
        since,
        counts: Counts {
            moments: data.moments.len(),
            media: data.media.len(),
            tags: data.tags.len(),
            files: files.len(),
            deletions: data.deletions.len(),
        },
    };

    let dir = backup_dir_for(pool, storage).await?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let name = format!(
        "{FILE_PREFIX}{}-{kind}.zip",
        now.format("%Y%m%d-%H%M%S")
    );
    let path = dir.join(&name);

    let root = storage.root().to_path_buf();
    let target = path.clone();
    tokio::task::spawn_blocking(move || write_zip(&target, &root, &manifest, &data, &files))
        .await
        .map_err(|e| AppError::Internal(e.into()))??;

    let size = tokio::fs::metadata(&path)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .len();

    // 包已经落地，才敢记状态
    save_state(
        pool,
        &BackupState {
            last_at: Some(now),
            base_name: if base_due {
                Some(name.clone())
            } else {
                state.base_name
            },
            base_at: if base_due { Some(now) } else { state.base_at },
            deltas: if base_due { 0 } else { state.deltas + 1 },
        },
    )
    .await?;

    if base_due {
        // 新的完整快照在手，旧链整个可以扔
        drop_old_chain(&dir, &name).await?;
        // 留痕也清了：快照本身就是真值
        crate::moments::repo::clear_deletions(pool).await?;
    }

    Ok(BackupFile {
        name,
        size_bytes: size,
        created_at: now,
        kind: kind.to_string(),
    })
}

/// 取数据快照与需要打进去的文件。
///
/// 快照永远是全量（几万行很小，增量反而容易漏）；文件按 since 筛——
/// 媒体文件一旦落盘就不再变，所以用 created_at 判断新增就够了。
async fn collect(
    pool: &PgPool,
    since: Option<DateTime<Utc>>,
) -> AppResult<(BackupData, Vec<String>)> {
    let moments = sqlx::query_as::<_, MomentBackup>(
        "SELECT id, title, content, happened_at, created_at, updated_at, \
                location_name, city, latitude, longitude \
         FROM moments ORDER BY happened_at",
    )
    .fetch_all(pool)
    .await?;

    let media = sqlx::query_as::<_, MediaBackup>(
        "SELECT id, moment_id, kind, role, file_path, thumb_path, mime_type, size_bytes, \
                width, height, duration_ms, position, created_at, tag_status, tagged_at \
         FROM media ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;

    let tags =
        sqlx::query_as::<_, TagBackup>("SELECT id, name, created_at FROM tags ORDER BY name")
            .fetch_all(pool)
            .await?;

    let moment_tags =
        sqlx::query_as::<_, MomentTagBackup>("SELECT moment_id, tag_id FROM moment_tags")
            .fetch_all(pool)
            .await?;

    let media_tags = sqlx::query_as::<_, MediaTagBackup>(
        "SELECT media_id, tag_key, tag_name, tag_group, score FROM media_tags",
    )
    .fetch_all(pool)
    .await?;

    // 基准包不带留痕：完整快照自己就是真值
    let deletions = if since.is_some() {
        crate::moments::repo::deletions_since(pool, since).await?
            .into_iter()
            .map(|(id, table_name, deleted_at)| DeletionBackup {
                id,
                table_name,
                deleted_at,
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut files: Vec<String> = Vec::new();
    for item in &media {
        let is_new = match since {
            Some(since) => item.created_at.map_or(true, |at| at > since),
            None => true,
        };
        if !is_new {
            continue;
        }
        files.push(item.file_path.clone());
        if let Some(thumb) = &item.thumb_path {
            files.push(thumb.clone());
        }
    }
    files.sort();
    files.dedup();

    Ok((
        BackupData {
            moments,
            media,
            tags,
            moment_tags,
            media_tags,
            deletions,
        },
        files,
    ))
}

/// 同步写 zip。媒体文件用 Stored（本来就是压缩格式），两个 json 用 Deflated。
fn write_zip(
    target: &Path,
    root: &Path,
    manifest: &Manifest,
    data: &BackupData,
    files: &[String],
) -> AppResult<()> {
    let temp = target.with_extension("zip.part");
    let file = std::fs::File::create(&temp).map_err(|e| AppError::Internal(e.into()))?;
    let mut zip = ZipWriter::new(file);

    // zip 默认把时间戳写成 1980，解压出来一排 1980 年的文件看着别扭，统一盖成导出时刻
    let stamp = zip_time(Utc::now());
    let packed = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(stamp);
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(stamp);

    zip.start_file(MANIFEST_NAME, packed)
        .map_err(|e| AppError::Internal(e.into()))?;
    zip.write_all(&to_json(manifest)?)
        .map_err(|e| AppError::Internal(e.into()))?;

    zip.start_file(DATA_NAME, packed)
        .map_err(|e| AppError::Internal(e.into()))?;
    zip.write_all(&serde_json::to_vec(data).map_err(|e| AppError::Internal(e.into()))?)
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut missing = 0usize;
    for rel in files {
        // 数据库里有、磁盘上没了的记录不该让整份备份失败，跳过并计数
        let Ok(mut source) = std::fs::File::open(root.join(rel)) else {
            missing += 1;
            continue;
        };
        zip.start_file(format!("{FILES_PREFIX}{rel}"), stored)
            .map_err(|e| AppError::Internal(e.into()))?;
        std::io::copy(&mut source, &mut zip).map_err(|e| AppError::Internal(e.into()))?;
    }

    zip.finish().map_err(|e| AppError::Internal(e.into()))?;

    // 写完整了再改名，免得中途失败留下一个看着像好包的残件
    std::fs::rename(&temp, target).map_err(|e| AppError::Internal(e.into()))?;

    if missing > 0 {
        tracing::warn!(missing, "有文件在磁盘上找不到，已从备份中略过");
    }
    Ok(())
}

fn to_json<T: Serialize>(value: &T) -> AppResult<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|e| AppError::Internal(e.into()))
}

/// zip 的时间戳只认 1980–2107，超范围就退回默认值
fn zip_time(when: DateTime<Utc>) -> zip::DateTime {
    use chrono::{Datelike, Timelike};
    let naive = when.naive_utc();
    zip::DateTime::from_date_and_time(
        naive.year() as u16,
        naive.month() as u8,
        naive.day() as u8,
        naive.hour() as u8,
        naive.minute() as u8,
        naive.second() as u8,
    )
    .unwrap_or_default()
}

// ---------------------------------------------------------------- 恢复

/// 从一份包里恢复。一次只吃一个包；多个包由上层按时间排序后依次调用。
///
/// 合并规则是「谁新听谁的」：
/// - 记忆按 `updated_at` 比，包里更新才覆盖——这样导入旧包不会把新内容冲掉
/// - 媒体按包里的关联关系覆盖（它没有时间戳，而包是串行的，后一个包说的算）
/// - 最后重放删除留痕：本地那条如果没比删除时刻更新，就删掉
pub async fn restore(
    pool: &PgPool,
    storage: &Storage,
    archive: &Path,
) -> AppResult<RestoreReport> {
    let root = storage.root().to_path_buf();
    let source = archive.to_path_buf();

    // 解包与写盘是阻塞活儿
    let unpacked = tokio::task::spawn_blocking(move || unpack(&source, &root))
        .await
        .map_err(|e| AppError::Internal(e.into()))??;

    let data = unpacked.data;
    let mut report = RestoreReport {
        files_written: unpacked.files_written,
        files_skipped: unpacked.files_skipped,
        ..Default::default()
    };

    let mut tx = pool.begin().await?;

    // 先重放删除，再灌数据。反过来的话，会把快照里刚恢复的又删掉。
    for item in &data.deletions {
        let affected = match item.table_name.as_str() {
            "moments" => sqlx::query("DELETE FROM moments WHERE id = $1 AND updated_at <= $2")
                .bind(item.id)
                .bind(item.deleted_at)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
            "media" => sqlx::query("DELETE FROM media WHERE id = $1")
                .bind(item.id)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
            other => {
                tracing::debug!(table = other, "留痕里有个不认识的表名，略过");
                0
            }
        };
        report.deletions_applied += affected;
    }

    for tag in &data.tags {
        let result = sqlx::query(
            "INSERT INTO tags (id, name, created_at) VALUES ($1, $2, COALESCE($3, now())) \
             ON CONFLICT DO NOTHING",
        )
        .bind(tag.id)
        .bind(&tag.name)
        .bind(tag.created_at)
        .execute(&mut *tx)
        .await?;
        report.tags_added += result.rows_affected();
    }

    for moment in &data.moments {
        // 只有包里那份更新才覆盖：导入旧包不该把本地的新内容冲掉
        let result = sqlx::query(
            "INSERT INTO moments \
                (id, title, content, happened_at, created_at, updated_at, \
                 location_name, city, latitude, longitude) \
             VALUES ($1, $2, $3, $4, COALESCE($5, now()), COALESCE($6, now()), $7, $8, $9, $10) \
             ON CONFLICT (id) DO UPDATE SET \
                title = EXCLUDED.title, \
                content = EXCLUDED.content, \
                happened_at = EXCLUDED.happened_at, \
                updated_at = EXCLUDED.updated_at, \
                location_name = EXCLUDED.location_name, \
                city = EXCLUDED.city, \
                latitude = EXCLUDED.latitude, \
                longitude = EXCLUDED.longitude \
             WHERE EXCLUDED.updated_at > moments.updated_at",
        )
        .bind(moment.id)
        .bind(&moment.title)
        .bind(&moment.content)
        .bind(moment.happened_at)
        .bind(moment.created_at)
        .bind(moment.updated_at)
        .bind(&moment.location_name)
        .bind(&moment.city)
        .bind(moment.latitude)
        .bind(moment.longitude)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() > 0 {
            report.moments_added += 1;
        } else {
            report.moments_skipped += 1;
        }
    }

    for item in &data.media {
        let result = sqlx::query(
            "INSERT INTO media \
                (id, moment_id, kind, role, file_path, thumb_path, mime_type, size_bytes, \
                 width, height, duration_ms, position, created_at, tag_status, tagged_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
                     COALESCE($13, now()), $14, $15) \
             ON CONFLICT (id) DO UPDATE SET \
                moment_id = EXCLUDED.moment_id, \
                role = EXCLUDED.role, \
                position = EXCLUDED.position",
        )
        .bind(item.id)
        .bind(item.moment_id)
        .bind(&item.kind)
        .bind(&item.role)
        .bind(&item.file_path)
        .bind(&item.thumb_path)
        .bind(&item.mime_type)
        .bind(item.size_bytes)
        .bind(item.width)
        .bind(item.height)
        .bind(item.duration_ms)
        .bind(item.position)
        .bind(item.created_at)
        .bind(if item.tag_status.is_empty() {
            "pending"
        } else {
            item.tag_status.as_str()
        })
        .bind(item.tagged_at)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() > 0 {
            report.media_added += 1;
        } else {
            report.media_skipped += 1;
        }
    }

    for link in &data.moment_tags {
        sqlx::query(
            "INSERT INTO moment_tags (moment_id, tag_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(link.moment_id)
        .bind(link.tag_id)
        .execute(&mut *tx)
        .await?;
    }

    for item in &data.media_tags {
        sqlx::query(
            "INSERT INTO media_tags (media_id, tag_key, tag_name, tag_group, score) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
        )
        .bind(item.media_id)
        .bind(&item.tag_key)
        .bind(&item.tag_name)
        .bind(&item.tag_group)
        .bind(item.score)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // 收进来的记忆如果还没有城市，顺手补一个——老备份都没有 city
    backfill_cities(pool).await?;

    Ok(report)
}

struct Unpacked {
    data: BackupData,
    files_written: u64,
    files_skipped: u64,
}

fn unpack(archive: &Path, root: &Path) -> AppResult<Unpacked> {
    let file = std::fs::File::open(archive).map_err(|e| AppError::Internal(e.into()))?;
    let mut zip =
        ZipArchive::new(file).map_err(|e| AppError::validation(format!("打不开这个包：{e}")))?;

    let manifest: Manifest = {
        let mut entry = zip
            .by_name(MANIFEST_NAME)
            .map_err(|_| AppError::validation("这不像是一份流光备份（缺 manifest.json）"))?;
        let mut text = Vec::new();
        entry
            .read_to_end(&mut text)
            .map_err(|e| AppError::Internal(e.into()))?;
        serde_json::from_slice(&text)
            .map_err(|e| AppError::validation(format!("备份信息读不出来：{e}")))?
    };

    if manifest.format_version > FORMAT_VERSION {
        return Err(AppError::validation(format!(
            "这份备份是更新版本（v{}）导出的，当前只认到 v{FORMAT_VERSION}",
            manifest.format_version
        )));
    }

    let data: BackupData = {
        let mut entry = zip
            .by_name(DATA_NAME)
            .map_err(|_| AppError::validation("备份里没有 data.json"))?;
        let mut text = Vec::new();
        entry
            .read_to_end(&mut text)
            .map_err(|e| AppError::Internal(e.into()))?;
        serde_json::from_slice(&text)
            .map_err(|e| AppError::validation(format!("备份数据读不出来：{e}")))?
    };

    let mut files_written = 0u64;
    let mut files_skipped = 0u64;

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| AppError::Internal(e.into()))?;
        let Some(rel) = entry.name().strip_prefix(FILES_PREFIX) else {
            continue;
        };
        // 路径必须干干净净：任何 .. 或绝对路径都不接受，免得写穿数据目录
        let Some(rel) = safe_relative(rel) else {
            tracing::warn!(name = entry.name(), "备份里有可疑路径，已略过");
            continue;
        };
        let abs = root.join(&rel);
        if abs.exists() {
            files_skipped += 1;
            continue;
        }
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Internal(e.into()))?;
        }
        let mut target = std::fs::File::create(&abs).map_err(|e| AppError::Internal(e.into()))?;
        std::io::copy(&mut entry, &mut target).map_err(|e| AppError::Internal(e.into()))?;
        files_written += 1;
    }

    Ok(Unpacked {
        data,
        files_written,
        files_skipped,
    })
}

/// 只放行 `a/b/c.jpg` 这种相对路径
fn safe_relative(raw: &str) -> Option<PathBuf> {
    let path = Path::new(raw);
    if path.is_absolute() {
        return None;
    }
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::Normal(seg) => clean.push(seg),
            _ => return None,
        }
    }
    if clean.as_os_str().is_empty() {
        None
    } else {
        Some(clean)
    }
}

/// 给没有城市的记忆补上城市。开着定位表就能算，算不出就留空。
async fn backfill_cities(pool: &PgPool) -> AppResult<u64> {
    let rows = sqlx::query_as::<_, (Uuid, f64, f64)>(
        "SELECT id, latitude, longitude FROM moments \
         WHERE city = '' AND latitude IS NOT NULL AND longitude IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;

    let mut filled = 0u64;
    for (id, latitude, longitude) in rows {
        let Some(city) = crate::geo::nearest_city(latitude, longitude) else {
            continue;
        };
        sqlx::query("UPDATE moments SET city = $2 WHERE id = $1")
            .bind(id)
            .bind(city)
            .execute(pool)
            .await?;
        filled += 1;
    }

    if filled > 0 {
        tracing::info!(filled, "导入的记忆补上了城市");
    }
    Ok(filled)
}

// ---------------------------------------------------------------- 备份文件管理

/// 从文件名认出这是基准还是增量
fn kind_of(name: &str) -> String {
    if name.contains("-delta") {
        KIND_DELTA.to_string()
    } else {
        KIND_BASE.to_string()
    }
}

/// 列出已有的备份，新的在前
pub async fn list(dir: &Path) -> AppResult<Vec<BackupFile>> {
    let mut files = Vec::new();

    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        // 还没备份过就不算错
        Err(_) => return Ok(files),
    };

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(FILE_PREFIX) || !name.ends_with(".zip") {
            continue;
        }
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        let created_at = meta
            .modified()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());
        files.push(BackupFile {
            kind: kind_of(&name),
            name,
            size_bytes: meta.len(),
            created_at,
        });
    }

    // 文件名的日期戳本身就是排序依据
    files.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(files)
}

/// 找出某个备份在磁盘上的位置，顺带挡住路径穿越
pub fn resolve(dir: &Path, name: &str) -> AppResult<PathBuf> {
    let name = name.rsplit('/').next().unwrap_or(name);
    if !name.starts_with(FILE_PREFIX) || !name.ends_with(".zip") {
        return Err(AppError::validation("这个文件名不对劲"));
    }
    if safe_relative(name).is_none() {
        return Err(AppError::validation("这个文件名不对劲"));
    }
    Ok(dir.join(name))
}

/// 新基准落地后，把旧链整个清掉——新的基准是完整快照，旧包不再需要。
///
/// 注意只能在**新基准成功写盘之后**调用：早一步就把唯一的退路也删了。
async fn drop_old_chain(dir: &Path, keep_name: &str) -> AppResult<u64> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return Ok(0),
    };

    let mut removed = 0u64;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(FILE_PREFIX) || !name.ends_with(".zip") || name == keep_name {
            continue;
        }
        if tokio::fs::remove_file(entry.path()).await.is_ok() {
            removed += 1;
            tracing::info!(name = %name, "旧链已清理");
        }
    }
    Ok(removed)
}

// ---------------------------------------------------------------- 定时

/// 定时备份。
///
/// 间隔是按天配置的（设置页里改，存库），所以这里不摆一个「N 天后的定时器」，
/// 而是每小时醒一次问一句「该备了吗」——间隔改了立刻就生效，不用重启。
pub fn spawn_scheduler(pool: PgPool, storage: Storage) {
    tokio::spawn(async move {
        if let Err(err) = tick(&pool, &storage).await {
            tracing::warn!(error = %err, "启动时的备份检查没通过");
        }

        let mut ticker = tokio::time::interval(Duration::hours(1).to_std().unwrap());
        ticker.tick().await;

        loop {
            ticker.tick().await;
            if let Err(err) = tick(&pool, &storage).await {
                tracing::warn!(error = %err, "备份检查没通过");
            }
        }
    });
}

async fn tick(pool: &PgPool, storage: &Storage) -> AppResult<()> {
    let days = interval_days(pool).await?;
    // 0 表示关掉自动备份，只手动导
    if days == 0 {
        return Ok(());
    }

    let state = load_state(pool).await?;
    if let Some(last) = state.last_at {
        let hours_since = (Utc::now() - last).num_hours();
        if hours_since < days * 24 {
            return Ok(());
        }
    }

    run_once(pool, storage).await
}

/// 打一份包。基准包会顺手清掉旧链，所以不必另外打扫。
async fn run_once(pool: &PgPool, storage: &Storage) -> AppResult<()> {
    let started = std::time::Instant::now();
    let file = create(pool, storage).await?;

    // 耗时基本就是「把新增的媒体写一份」。吞吐量比绝对秒数更有参考价值。
    let seconds = started.elapsed().as_secs_f64().max(0.001);
    let mb_per_second = (file.size_bytes as f64 / 1_048_576.0) / seconds;

    tracing::info!(
        name = %file.name,
        kind = %file.kind,
        size = file.size_bytes,
        seconds = format!("{seconds:.1}"),
        mb_per_second = format!("{mb_per_second:.0}"),
        "备份完成"
    );
    Ok(())
}

// ---------------------------------------------------------------- HTTP 接口

pub async fn api_list(State(state): State<AppState>) -> AppResult<Json<Vec<BackupFile>>> {
    let dir = backup_dir_for(&state.pool, &state.storage).await?;
    Ok(Json(list(&dir).await?))
}

/// 手动导出：现打一份包。手动导出不改变链的状态时也照打——
/// 它是「现在就要一份」，和定时任务的节奏是两回事。
pub async fn api_create(State(state): State<AppState>) -> AppResult<Json<BackupFile>> {
    let file = create(&state.pool, &state.storage).await?;
    tracing::info!(name = %file.name, kind = %file.kind, size = file.size_bytes, "手动导出完成");
    Ok(Json(file))
}

pub async fn api_download(
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
) -> AppResult<Response> {
    let dir = backup_dir_for(&state.pool, &state.storage).await?;
    let path = resolve(&dir, &name)?;
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| AppError::not_found("备份", name.clone()))?;

    // 流式吐出去，别把整个包读进内存
    let stream = tokio_util::io::ReaderStream::new(file);
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "backup.zip".to_string());

    Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from_stream(stream))
        .map_err(|e| AppError::Internal(e.into()))
}

async fn settings_of(pool: &PgPool, storage: &Storage) -> AppResult<BackupSettings> {
    Ok(BackupSettings {
        interval_days: interval_days(pool).await?,
        dir: backup_dir_for(pool, storage).await?.to_string_lossy().to_string(),
        default_dir: default_dir(storage).to_string_lossy().to_string(),
    })
}

pub async fn api_settings(State(state): State<AppState>) -> AppResult<Json<BackupSettings>> {
    Ok(Json(settings_of(&state.pool, &state.storage).await?))
}

pub async fn api_set_settings(
    State(state): State<AppState>,
    Json(input): Json<BackupSettingsInput>,
) -> AppResult<Json<BackupSettings>> {
    let days = set_interval_days(&state.pool, input.interval_days).await?;
    tracing::info!(days, "自动备份间隔已更新");

    if let Some(raw) = input.dir.as_deref() {
        let current = backup_dir_for(&state.pool, &state.storage).await?;
        let next = prepare_dir(raw, &default_dir(&state.storage)).await?;

        if next != current {
            // 先把包搬过去，再落设置：搬不动就整个回退，链还是完整的
            let (moved, failed) = migrate_archives(&current, &next).await?;
            if failed > 0 {
                return Err(AppError::validation(format!(
                    "有 {failed} 个包没能搬过去，目录就先不改了。检查一下权限或磁盘空间"
                )));
            }
            save_dir(&state.pool, &next).await?;
            tracing::info!(dir = %next.display(), moved, "备份目录已更新");
        }
    }

    Ok(Json(settings_of(&state.pool, &state.storage).await?))
}

/// 导入：先把上传的包逐个落到临时文件，再按文件名里的时间戳从旧到新依次重放。
///
/// 一次可以给多个（一个基准 + 若干增量），这正是增量链恢复的方式。
/// 其中某个包坏了不会让整批停下——能吃的先吃进去，坏的那个记在 failed 里带回去。
pub async fn api_restore(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<Json<RestoreReport>> {
    let dir = backup_dir_for(&state.pool, &state.storage).await?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut staged: Vec<(String, PathBuf)> = Vec::new();

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::validation(format!("上传中断：{e}")))?
    {
        let looks_like_file = field.name() == Some("file") || field.file_name().is_some();
        if !looks_like_file {
            continue;
        }

        let original = field
            .file_name()
            .map(|name| name.rsplit('/').next().unwrap_or(name).to_string())
            .unwrap_or_else(|| format!("backup-{}.zip", staged.len()));

        let temp = dir.join(format!("import-{}-{}.zip", Uuid::new_v4(), staged.len()));

        let mut size: u64 = 0;
        {
            let mut file = tokio::fs::File::create(&temp)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;

            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        size += chunk.len() as u64;
                        if size > MAX_IMPORT_BYTES {
                            drop(file);
                            let _ = tokio::fs::remove_file(&temp).await;
                            cleanup_staged(&staged).await;
                            return Err(AppError::PayloadTooLarge(
                                "这个包太大了，超过 32G".to_string(),
                            ));
                        }
                        file.write_all(&chunk)
                            .await
                            .map_err(|e| AppError::Internal(e.into()))?;
                    }
                    Ok(None) => break,
                    Err(err) => {
                        drop(file);
                        let _ = tokio::fs::remove_file(&temp).await;
                        cleanup_staged(&staged).await;
                        return Err(AppError::validation(format!("上传中断：{err}")));
                    }
                }
            }
            file.flush().await.map_err(|e| AppError::Internal(e.into()))?;
        }

        if size == 0 {
            let _ = tokio::fs::remove_file(&temp).await;
            continue;
        }

        staged.push((original, temp));
    }

    if staged.is_empty() {
        return Err(AppError::validation("没有收到备份文件"));
    }

    // 按原始文件名排（里面带着日期戳），从旧到新依次重放——
    // 增量的数据快照要盖在前一个之上，顺序反了结果就不对
    staged.sort_by(|a, b| a.0.cmp(&b.0));

    let mut total = RestoreReport::default();
    for (original, path) in &staged {
        match restore(&state.pool, &state.storage, path).await {
            Ok(report) => {
                tracing::info!(
                    file = %original,
                    moments = report.moments_added,
                    skipped = report.moments_skipped,
                    deletions = report.deletions_applied,
                    files = report.files_written,
                    "这个包吃下了"
                );
                total.moments_added += report.moments_added;
                total.moments_skipped += report.moments_skipped;
                total.media_added += report.media_added;
                total.media_skipped += report.media_skipped;
                total.tags_added += report.tags_added;
                total.files_written += report.files_written;
                total.files_skipped += report.files_skipped;
                total.deletions_applied += report.deletions_applied;
            }
            Err(err) => {
                tracing::warn!(file = %original, error = %err, "这个包没吃下");
                total.failed.push(format!("{original}：{err}"));
            }
        }
    }

    cleanup_staged(&staged).await;
    Ok(Json(total))
}

/// 收尾：临时文件一个都不留，免得 backups 里攒下一堆半截的包
async fn cleanup_staged(staged: &[(String, PathBuf)]) {
    for (_, path) in staged {
        let _ = tokio::fs::remove_file(path).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_only() {
        assert!(safe_relative("2026/09/15/a.jpg").is_some());
        assert!(safe_relative("../escape.jpg").is_none());
        assert!(safe_relative("/etc/passwd").is_none());
        assert!(safe_relative("").is_none());
    }

    #[test]
    fn tells_base_from_delta_by_name() {
        assert_eq!(kind_of("efflux-20260915-103000-base.zip"), "base");
        assert_eq!(kind_of("efflux-20260916-103000-delta.zip"), "delta");
        // 老包没有后缀，按基准算（那时候的包本来就是全量）
        assert_eq!(kind_of("efflux-20260915-103000.zip"), "base");
    }

    #[test]
    fn backup_name_must_look_right() {
        assert!(resolve_name_ok("efflux-20260915-103000-base.zip"));
        assert!(!resolve_name_ok("../../etc/passwd"));
        assert!(!resolve_name_ok("something.zip"));
    }

    fn resolve_name_ok(name: &str) -> bool {
        name.starts_with(FILE_PREFIX) && name.ends_with(".zip") && safe_relative(name).is_some()
    }
}

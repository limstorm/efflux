use std::path::{Path, PathBuf};

use axum::extract::multipart::Field;
use chrono::{DateTime, Datelike, Utc};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// 媒体种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
    Audio,
}

impl Kind {
    pub fn parse(raw: &str) -> AppResult<Self> {
        match raw {
            "image" => Ok(Kind::Image),
            "video" => Ok(Kind::Video),
            "audio" => Ok(Kind::Audio),
            other => Err(AppError::Unsupported(format!("未知的媒体种类: {other}"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::Audio => "audio",
        }
    }
}

/// 白名单：MIME → 扩展名。绝不使用客户端文件名，避免路径穿越与伪装。
const TYPES: &[(&str, &str, Kind)] = &[
    // 图片
    ("image/jpeg", "jpg", Kind::Image),
    ("image/pjpeg", "jpg", Kind::Image),
    ("image/png", "png", Kind::Image),
    ("image/webp", "webp", Kind::Image),
    ("image/gif", "gif", Kind::Image),
    ("image/avif", "avif", Kind::Image),
    ("image/heic", "heic", Kind::Image),
    ("image/heif", "heic", Kind::Image),
    ("image/bmp", "bmp", Kind::Image),
    ("image/tiff", "tiff", Kind::Image),
    // 视频
    ("video/mp4", "mp4", Kind::Video),
    ("video/quicktime", "mov", Kind::Video),
    ("video/webm", "webm", Kind::Video),
    ("video/x-matroska", "mkv", Kind::Video),
    ("video/x-msvideo", "avi", Kind::Video),
    ("video/mpeg", "mpeg", Kind::Video),
    ("video/3gpp", "3gp", Kind::Video),
    // 音频
    ("audio/mpeg", "mp3", Kind::Audio),
    ("audio/mp3", "mp3", Kind::Audio),
    ("audio/mp4", "m4a", Kind::Audio),
    ("audio/x-m4a", "m4a", Kind::Audio),
    ("audio/aac", "aac", Kind::Audio),
    ("audio/ogg", "ogg", Kind::Audio),
    ("audio/opus", "opus", Kind::Audio),
    ("audio/wav", "wav", Kind::Audio),
    ("audio/x-wav", "wav", Kind::Audio),
    ("audio/wave", "wav", Kind::Audio),
    ("audio/flac", "flac", Kind::Audio),
    ("audio/x-flac", "flac", Kind::Audio),
    ("audio/webm", "weba", Kind::Audio),
];

fn lookup(mime: &str, kind: Kind) -> Option<&'static str> {
    TYPES
        .iter()
        .find(|(m, _, k)| *m == mime && *k == kind)
        .map(|(_, ext, _)| *ext)
}

#[derive(Debug, Clone)]
pub struct SavedFile {
    pub rel_path: String,
    pub size_bytes: i64,
    pub mime_type: String,
}

#[derive(Clone)]
pub struct Storage {
    root: PathBuf,
    max_bytes: usize,
}

impl Storage {
    pub fn new(root: PathBuf, max_bytes: usize) -> Self {
        Self { root, max_bytes }
    }

    pub async fn ensure_ready(&self) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 把上传字段流式写入磁盘（不整份进内存），并限制大小。
    pub async fn save_field(
        &self,
        field: &mut Field<'_>,
        kind: Kind,
        now: DateTime<Utc>,
        tag: Option<&str>,
    ) -> AppResult<SavedFile> {
        let mime = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_ascii_lowercase();
        let mime = mime.split(';').next().unwrap_or("").trim().to_string();

        let ext = lookup(&mime, kind).ok_or_else(|| {
            AppError::Unsupported(format!("暂时不支持这种文件格式（{mime}）"))
        })?;

        let filename = match tag {
            Some(tag) => format!("{}-{}.{}", Uuid::new_v4(), tag, ext),
            None => format!("{}.{}", Uuid::new_v4(), ext),
        };
        let rel_path = format!(
            "{}/{:02}/{:02}/{}",
            now.year(),
            now.month(),
            now.day(),
            filename
        );
        let abs_path = self.root.join(&rel_path);

        if let Some(parent) = abs_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
        }

        let mut file = tokio::fs::File::create(&abs_path)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

        // 视频不设大小上限：一段素材动辄几个 G，卡着限额只会把真正想记的东西挡在门外。
        // 图片与音频仍旧守着 max_bytes —— 它们体积可控，超了多半是选错了文件。
        let limit = match kind {
            Kind::Video => None,
            _ => Some(self.max_bytes as i64),
        };

        let mut size: i64 = 0;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    size += chunk.len() as i64;
                    if let Some(limit) = limit {
                        if size > limit {
                            drop(file);
                            let _ = tokio::fs::remove_file(&abs_path).await;
                            return Err(AppError::PayloadTooLarge(format!(
                                "文件太大了，单个文件请控制在 {} MB 以内",
                                self.max_bytes / 1024 / 1024
                            )));
                        }
                    }
                    file.write_all(&chunk)
                        .await
                        .map_err(|e| AppError::Internal(e.into()))?;
                }
                Ok(None) => break,
                Err(err) => {
                    drop(file);
                    let _ = tokio::fs::remove_file(&abs_path).await;
                    return Err(AppError::validation(format!("文件上传中断：{err}")));
                }
            }
        }

        file.flush().await.map_err(|e| AppError::Internal(e.into()))?;
        drop(file);

        if size == 0 {
            let _ = tokio::fs::remove_file(&abs_path).await;
            return Err(AppError::validation("这是一个空文件"));
        }

        Ok(SavedFile {
            rel_path,
            size_bytes: size,
            mime_type: mime,
        })
    }

    pub async fn remove(&self, rel_path: &str) {
        let abs = self.root.join(rel_path);
        if let Err(err) = tokio::fs::remove_file(&abs).await {
            tracing::warn!(path = %abs.display(), error = %err, "删除文件失败");
        }
    }
}

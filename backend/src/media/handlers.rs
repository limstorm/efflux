use axum::extract::multipart::Multipart;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

use super::repo::{self, NewMedia};
use super::storage::{Kind, SavedFile};
use crate::error::{AppError, AppResult};
use crate::moments::models::MediaDto;
use crate::state::AppState;

/// `POST /api/media/upload/{kind}`  kind = image | video | audio
///
/// multipart 字段：
/// - `file`（必需）主文件
/// - `thumb`（可选）前端生成的缩略图，仅图片/视频
/// - `width` / `height` / `durationMs`（可选）元数据
pub async fn upload(
    State(state): State<AppState>,
    Path(kind): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Json<MediaDto>> {
    let kind = Kind::parse(&kind)?;
    let storage = state.storage.clone();
    let now = Utc::now();

    let mut main: Option<SavedFile> = None;
    let mut thumb: Option<SavedFile> = None;
    let mut width: Option<i32> = None;
    let mut height: Option<i32> = None;
    let mut duration_ms: Option<i32> = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::validation(format!("上传数据不完整：{err}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        let result = match name.as_str() {
            "file" => {
                let saved = storage.save_field(&mut field, kind, now, None).await?;
                main = Some(saved);
                Ok(())
            }
            "thumb" => {
                if kind == Kind::Audio {
                    Ok(())
                } else {
                    let saved = storage
                        .save_field(&mut field, Kind::Image, now, Some("thumb"))
                        .await?;
                    thumb = Some(saved);
                    Ok(())
                }
            }
            "width" => {
                width = parse_num(field).await;
                Ok(())
            }
            "height" => {
                height = parse_num(field).await;
                Ok(())
            }
            "durationMs" => {
                duration_ms = parse_num(field).await;
                Ok(())
            }
            _ => Ok(()),
        };

        if let Err(err) = result {
            cleanup(&storage, main.as_ref(), thumb.as_ref()).await;
            return Err(err);
        }
    }

    let Some(main) = main else {
        cleanup(&storage, None, thumb.as_ref()).await;
        return Err(AppError::validation("没有收到文件，请重新选择"));
    };

    let new_media = NewMedia {
        kind: kind.as_str().to_string(),
        role: if kind == Kind::Audio { "music" } else { "attachment" }.to_string(),
        file_path: main.rel_path.clone(),
        thumb_path: thumb.as_ref().map(|t| t.rel_path.clone()),
        mime_type: main.mime_type.clone(),
        size_bytes: main.size_bytes,
        width,
        height,
        duration_ms,
    };

    match repo::insert(&state.pool, new_media).await {
        Ok(row) => {
            // 开了自动打标就把这张媒体排进队列：图片用原图认，视频用前端抓的
            // 首帧缩略图认。排不进去也不影响上传本身，用户拿到的是完整的媒体信息。
            if (kind == Kind::Image || kind == Kind::Video) && state.tagger.is_some() {
                if let Err(err) =
                    sqlx::query("UPDATE media SET tag_status = 'pending' WHERE id = $1")
                        .bind(row.id)
                        .execute(&state.pool)
                        .await
                {
                    tracing::warn!(media = %row.id, error = %err, "没能把图片排进打标队列");
                }
            }
            Ok(Json(MediaDto::from(row)))
        }
        Err(err) => {
            cleanup(&storage, Some(&main), thumb.as_ref()).await;
            Err(err)
        }
    }
}

/// `DELETE /api/media/{id}` 仅删除尚未被使用的媒体
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let media = repo::find(&state.pool, id).await?;
    let Some(media) = media else {
        return Err(AppError::not_found("媒体", id.to_string()));
    };
    if media.moment_id.is_some() {
        return Err(AppError::validation("这段媒体正在被使用，请先移除它所属的点滴"));
    }

    let deleted = repo::delete_orphan(&state.pool, id).await?;
    if !deleted {
        return Err(AppError::validation("这段媒体正在被使用"));
    }

    state.storage.remove(&media.file_path).await;
    if let Some(thumb) = media.thumb_path.as_deref() {
        state.storage.remove(thumb).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn parse_num(field: axum::extract::multipart::Field<'_>) -> Option<i32> {
    field
        .text()
        .await
        .ok()
        .and_then(|raw| raw.trim().parse::<f64>().ok())
        .map(|v| v.round() as i32)
        .filter(|v| *v >= 0)
}

async fn cleanup(
    storage: &super::storage::Storage,
    main: Option<&SavedFile>,
    thumb: Option<&SavedFile>,
) {
    if let Some(main) = main {
        storage.remove(&main.rel_path).await;
    }
    if let Some(thumb) = thumb {
        storage.remove(&thumb.rel_path).await;
    }
}

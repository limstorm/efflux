//! 打标流水线。
//!
//! 串行地处理 `tag_status = 'pending'` 的图片：推理是 CPU 密集的，放进
//! `spawn_blocking` 里跑，别占着 tokio 的工作线程；一次只处理一张，
//! 免得把整台机器的 CPU 吃满。
//!
//! 想给已有图片补标，把状态重置一下即可：
//!   UPDATE media SET tag_status = 'pending' WHERE kind = 'image';

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use super::clip::{TagHit, Tagger};
use crate::media::Storage;

/// 一轮没活干就睡一会儿
const IDLE_SLEEP: Duration = Duration::from_secs(15);
/// 一轮最多处理几张
const BATCH: i64 = 4;

pub fn spawn(pool: PgPool, storage: Storage, tagger: Arc<Mutex<Tagger>>) {
    tokio::spawn(async move {
        let label_count = tagger.lock().map(|guard| guard.label_count()).unwrap_or(0);
        tracing::info!(labels = label_count, "自动打标已启动");

        // 老库升上来时存量图片还没有向量，先补一遍
        if let Err(err) = backfill_embeddings(&pool, &storage, tagger.clone()).await {
            tracing::warn!(error = %err, "补算向量失败，下次启动再试");
        }

        loop {
            match process_batch(&pool, &storage, tagger.clone()).await {
                Ok(0) => tokio::time::sleep(IDLE_SLEEP).await,
                Ok(count) => tracing::debug!(count, "这一批打标完成"),
                Err(err) => {
                    tracing::warn!(error = %err, "打标出错，稍后再试");
                    tokio::time::sleep(IDLE_SLEEP).await;
                }
            }
        }
    });
}

async fn process_batch(
    pool: &PgPool,
    storage: &Storage,
    tagger: Arc<Mutex<Tagger>>,
) -> anyhow::Result<usize> {
    let rows: Vec<(Uuid, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, kind, file_path, thumb_path FROM media \
         WHERE kind IN ('image', 'video') AND tag_status = 'pending' \
         ORDER BY created_at ASC LIMIT $1",
    )
    .bind(BATCH)
    .fetch_all(pool)
    .await?;

    let count = rows.len();

    for (id, kind, file_path, thumb_path) in rows {
        // 图片直接用原图；视频没有原图可解，用前端抓的首帧缩略图
        let source = if kind == "video" {
            match thumb_path {
                Some(thumb) => thumb,
                None => {
                    tracing::warn!(media = %id, "这个视频没有缩略图，跳过打标");
                    mark_failed(pool, id).await;
                    continue;
                }
            }
        } else {
            file_path
        };

        let abs_path = storage.root().join(&source);
        let worker = tagger.clone();

        // 推理是同步的 CPU 活，扔到阻塞线程池；锁在闭包内用完即放
        let outcome = tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<TagHit>, Vec<f32>)> {
            let mut guard = worker
                .lock()
                .map_err(|_| anyhow::anyhow!("打标器状态异常"))?;
            Ok(guard.tag_image(&abs_path)?)
        })
        .await;

        match outcome {
            Ok(Ok((hits, embed))) => {
                if let Err(err) = save_tags(pool, id, &hits, &embed).await {
                    tracing::warn!(media = %id, error = %err, "标签写库失败");
                    mark_failed(pool, id).await;
                } else {
                    tracing::debug!(media = %id, tags = hits.len(), dim = embed.len(), "打标完成");
                }
            }
            Ok(Err(err)) => {
                tracing::warn!(media = %id, error = %err, "这张图没认出来");
                mark_failed(pool, id).await;
            }
            Err(err) => {
                tracing::warn!(media = %id, error = %err, "打标任务中断");
                mark_failed(pool, id).await;
            }
        }
    }

    Ok(count)
}

async fn save_tags(
    pool: &PgPool,
    media_id: Uuid,
    hits: &[TagHit],
    embed: &[f32],
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM media_tags WHERE media_id = $1")
        .bind(media_id)
        .execute(&mut *tx)
        .await?;

    // 向量一并留下，之后「找相似」直接用，不必再推理一遍
    sqlx::query(
        "INSERT INTO media_embeddings (media_id, dim, model, vec) VALUES ($1, $2, 'clip', $3) \
         ON CONFLICT (media_id) DO UPDATE SET dim = EXCLUDED.dim, model = EXCLUDED.model, \
                                              vec = EXCLUDED.vec, created_at = now()",
    )
    .bind(media_id)
    .bind(embed.len() as i32)
    .bind(crate::embedding::to_bytes(embed))
    .execute(&mut *tx)
    .await?;

    for hit in hits {
        sqlx::query(
            "INSERT INTO media_tags (media_id, tag_key, tag_name, tag_group, score) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(media_id)
        .bind(&hit.key)
        .bind(&hit.name)
        .bind(&hit.group)
        .bind(hit.score)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("UPDATE media SET tag_status = 'done', tagged_at = now() WHERE id = $1")
        .bind(media_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await
}

/// 给还没有向量的图片补算一遍。
///
/// 只在启动时跑：要处理的是「打标早就开着、但那时还不存向量」的老库。
/// 新图在上传打标时就顺手算了，用不着这里。顺带把标签也刷新一遍，
/// save_tags 本身是幂等的。
async fn backfill_embeddings(
    pool: &PgPool,
    storage: &Storage,
    tagger: Arc<Mutex<Tagger>>,
) -> anyhow::Result<()> {
    // 视频没有原图可解，用它上传时抓的首帧缩略图，跟打标时一个路子
    let rows: Vec<(Uuid, String, String, Option<String>)> = sqlx::query_as(
        "SELECT m.id, m.kind, m.file_path, m.thumb_path FROM media m \
         LEFT JOIN media_embeddings e ON e.media_id = m.id \
         WHERE m.kind IN ('image', 'video') AND e.media_id IS NULL \
           AND m.tag_status <> 'failed' \
         ORDER BY m.created_at LIMIT 500",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    tracing::info!(count = rows.len(), "开始给存量图片补算向量");
    let mut done = 0usize;

    for (id, kind, file_path, thumb_path) in rows {
        let source = if kind == "video" {
            match thumb_path {
                Some(thumb) => thumb,
                // 没有首帧就解不开，跳过
                None => continue,
            }
        } else {
            file_path
        };

        let abs = storage.root().join(&source);
        let worker = tagger.clone();

        let outcome =
            tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<TagHit>, Vec<f32>)> {
                let mut guard = worker
                    .lock()
                    .map_err(|_| anyhow::anyhow!("打标器状态异常"))?;
                Ok(guard.tag_image(&abs)?)
            })
            .await;

        match outcome {
            Ok(Ok((hits, embed))) => {
                if let Err(err) = save_tags(pool, id, &hits, &embed).await {
                    tracing::debug!(media = %id, error = %err, "补算写库失败");
                } else {
                    done += 1;
                }
            }
            Ok(Err(err)) => tracing::debug!(media = %id, error = %err, "这张图补算失败"),
            Err(err) => tracing::warn!(media = %id, error = %err, "补算任务中断"),
        }
    }

    tracing::info!(done, "存量图片的向量补算完成");
    Ok(())
}

async fn mark_failed(pool: &PgPool, media_id: Uuid) {
    // 标成 failed 而不是退回 pending，免得一张坏图解不开就无限重试
    let _ = sqlx::query("UPDATE media SET tag_status = 'failed', tagged_at = now() WHERE id = $1")
        .bind(media_id)
        .execute(pool)
        .await;
}

mod auth;
mod backup;
mod config;
mod db;
mod embedding;
mod error;
mod gallery;
mod geo;
mod map;
mod media;
mod moments;
mod ratelimit;
mod router;
mod settings;
mod state;
mod tagger;
mod tags;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::PgPool;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::Config;
use crate::media::Storage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    init_tracing();

    let config = Arc::new(Config::from_env()?);
    tracing::info!(
        port = config.port,
        data_dir = %config.data_dir.display(),
        "流光服务启动中"
    );

    let storage = Storage::new(config.data_dir.clone(), config.max_upload_bytes);
    storage.ensure_ready().await?;

    // 单机部署才会设这一项；设了却找不到 index.html 就先说一声，别等打开网页才发现
    if let Some(dir) = config.static_dir.as_deref() {
        if dir.join("index.html").is_file() {
            tracing::info!(dir = %dir.display(), "顺带托管前端静态文件");
        } else {
            tracing::warn!(
                dir = %dir.display(),
                "EFFLUX_STATIC_DIR 里没有 index.html，网页可能打不开（先构建前端）"
            );
        }
    }

    let pool = db::connect_with_retry(&config).await?;
    db::migrate(&pool).await?;
    tracing::info!("数据库已就绪");

    spawn_orphan_janitor(pool.clone(), storage.clone());

    // 定时备份。间隔在设置页里改（存库），所以这里只是把循环跑起来
    backup::spawn_scheduler(pool.clone(), storage.clone());

    let access = auth::AccessControl::bootstrap(
        &pool,
        config.access_code.as_deref(),
        config.session_days,
    )
    .await?;

    if access.is_protected() {
        tracing::info!(
            session_days = config.session_days,
            cookie_secure = config.cookie_secure,
            "已开启访问口令保护（可在站点设置里随时修改）"
        );
    } else {
        tracing::warn!("未设置 EFFLUX_ACCESS_CODE，任何人都能访问 —— 请不要这样放到公网");
    }

    let state = state::AppState::new(pool.clone(), config.clone(), storage, access);

    // 自动打标：模型有问题也不拦着服务启动，只是这块功能不生效
    let state = if config.tagger_enabled {
        match tagger::Labels::load()
            .and_then(|labels| tagger::Tagger::load(&config.model_dir, labels))
        {
            Ok(tagger) => {
                let tagger = Arc::new(Mutex::new(tagger));
                tagger::spawn_tagger(pool.clone(), state.storage.clone(), tagger.clone());
                state.with_tagger(tagger)
            }
            Err(err) => {
                tracing::error!(error = %err, "自动打标未能启动，图片不会被识别");
                state
            }
        }
    } else {
        tracing::info!("未开启图片自动识别（EFFLUX_TAGGER=1 可启用）");
        state
    };

    let app = router::build(state);

    if config.rate_limit {
        tracing::info!(
            api_per_minute = ratelimit::API_PER_MINUTE,
            login_per_minute = ratelimit::LOGIN_PER_MINUTE,
            "已开启限流（Docker 部署里 Nginx 前面还有一道）"
        );
    } else {
        tracing::warn!("EFFLUX_RATE_LIMIT=off，限流已关闭");
    }

    let listener = TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!("已就绪 → http://0.0.0.0:{}", config.port);

    // 带上对端地址：限流要知道请求是从哪儿来的
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    tracing::info!("正在关闭服务，等待连接释放…");
    pool.close().await;
    tracing::info!("已安全退出");
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // ort 初始化时会把 BFCArena 的内存预留逐条打出来，很吵，压到 warn
        EnvFilter::new("info,tower_http=info,sqlx=warn,ort=warn")
    });

    let registry = tracing_subscriber::registry().with(filter);

    if std::env::var("EFFLUX_LOG_JSON").is_ok() {
        registry
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        registry
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_level(true),
            )
            .init();
    }
}

/// 定期回收「上传了文件但最终没有创建点滴」的孤儿媒体。
fn spawn_orphan_janitor(pool: PgPool, storage: Storage) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(6 * 3600));
        ticker.tick().await; // 启动瞬间不执行
        loop {
            ticker.tick().await;
            match moments::repo::purge_orphan_media(&pool, 24).await {
                Ok(orphans) => {
                    if !orphans.is_empty() {
                        tracing::info!(count = orphans.len(), "回收未使用的媒体");
                    }
                    for (file, thumb) in orphans {
                        storage.remove(&file).await;
                        if let Some(thumb) = thumb {
                            storage.remove(&thumb).await;
                        }
                    }
                }
                Err(err) => tracing::warn!(error = %err, "回收孤儿媒体失败"),
            }

            if let Err(err) = moments::repo::purge_unused_tags(&pool).await {
                tracing::warn!(error = %err, "清理无用标签失败");
            }
        }
    });
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("无法监听 Ctrl+C 信号");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("无法监听 SIGTERM 信号")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("收到关闭信号");
}

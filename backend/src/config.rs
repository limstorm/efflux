use std::path::PathBuf;

/// 全部运行时配置来自环境变量，启动时集中校验，缺失即失败（fail fast）。
#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub data_dir: PathBuf,
    /// 单个文件上传上限（字节）。只管图片与音频 —— 视频不设上限
    pub max_upload_bytes: usize,
    pub max_connections: u32,
    pub cors_origins: Vec<String>,
    /// 家庭访问口令。留空表示不做鉴权（只适合本机开发）。
    pub access_code: Option<String>,
    /// 登录状态保持天数
    pub session_days: i64,
    /// 是否只在 HTTPS 下发送会话 Cookie（公网部署应为 true）
    pub cookie_secure: bool,
    /// 是否开启图片自动打标签（需要模型文件）
    pub tagger_enabled: bool,
    /// CLIP 模型目录
    pub model_dir: PathBuf,
    /// 打包好的前端目录。留给「不用 Docker 的单机部署」：
    /// 设上之后，后端顺手把 SPA 也发出去（Docker 里这份活由 Nginx 干）。
    pub static_dir: Option<PathBuf>,
    /// 后端自带的限流（登录 + API）。Docker 部署里 Nginx 还有一道，这里是本地部署的那道。
    pub rate_limit: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = required("DATABASE_URL")?;
        let data_dir = std::env::var("EFFLUX_DATA_DIR")
            .unwrap_or_else(|_| "./data".to_string())
            .into();

        let max_upload_mb = optional("EFFLUX_MAX_UPLOAD_MB", "512")
            .parse::<usize>()
            .map_err(|e| anyhow::anyhow!("EFFLUX_MAX_UPLOAD_MB 不是合法数字: {e}"))?;

        let port = optional("EFFLUX_PORT", "8080")
            .parse::<u16>()
            .map_err(|e| anyhow::anyhow!("EFFLUX_PORT 不是合法端口: {e}"))?;

        let max_connections = optional("EFFLUX_DB_MAX_CONNECTIONS", "16")
            .parse::<u32>()
            .map_err(|e| anyhow::anyhow!("EFFLUX_DB_MAX_CONNECTIONS 不是合法数字: {e}"))?;

        let cors_origins = std::env::var("EFFLUX_CORS_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:5173".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let access_code = std::env::var("EFFLUX_ACCESS_CODE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(code) = access_code.as_deref() {
            if code.chars().count() < 6 {
                anyhow::bail!("EFFLUX_ACCESS_CODE 太短了，至少 6 位");
            }
        }

        let session_days = optional("EFFLUX_SESSION_DAYS", "30")
            .parse::<i64>()
            .map_err(|e| anyhow::anyhow!("EFFLUX_SESSION_DAYS 不是合法数字: {e}"))?
            .clamp(1, 365);

        let cookie_secure = matches!(
            optional("EFFLUX_COOKIE_SECURE", "false").to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        );

        let tagger_enabled = matches!(
            optional("EFFLUX_TAGGER", "false").to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        );

        let model_dir = std::env::var("EFFLUX_MODEL_DIR")
            .unwrap_or_else(|_| "./models".to_string())
            .into();

        let static_dir = std::env::var("EFFLUX_STATIC_DIR")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);

        let rate_limit = !matches!(
            optional("EFFLUX_RATE_LIMIT", "on").to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        );

        Ok(Self {
            port,
            database_url,
            data_dir,
            max_upload_bytes: max_upload_mb * 1024 * 1024,
            max_connections,
            cors_origins,
            access_code,
            session_days,
            cookie_secure,
            tagger_enabled,
            model_dir,
            static_dir,
            rate_limit,
        })
    }
}

fn required(name: &str) -> anyhow::Result<String> {
    std::env::var(name).map_err(|_| anyhow::anyhow!("缺少必需的环境变量: {name}"))
}

fn optional(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

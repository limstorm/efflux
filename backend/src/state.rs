use std::sync::{Arc, Mutex};

use sqlx::PgPool;

use crate::auth::AccessControl;
use crate::config::Config;
use crate::media::Storage;
use crate::ratelimit::Limiter;
use crate::tagger::Tagger;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub storage: Storage,
    pub access: AccessControl,
    /// 开了自动打标才有；为 None 时上传流程完全不受影响
    pub tagger: Option<Arc<Mutex<Tagger>>>,
    /// 整站 API 的令牌桶
    pub api_limit: Arc<Limiter>,
    /// 登录的令牌桶（更紧）
    pub login_limit: Arc<Limiter>,
}

impl AppState {
    pub fn new(pool: PgPool, config: Arc<Config>, storage: Storage, access: AccessControl) -> Self {
        Self {
            pool,
            config,
            storage,
            access,
            tagger: None,
            api_limit: Arc::new(Limiter::api()),
            login_limit: Arc::new(Limiter::login()),
        }
    }

    pub fn with_tagger(mut self, tagger: Arc<Mutex<Tagger>>) -> Self {
        self.tagger = Some(tagger);
        self
    }
}

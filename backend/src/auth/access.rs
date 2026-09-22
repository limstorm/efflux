use std::sync::{Arc, RwLock};

use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use sqlx::PgPool;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use super::token;
use crate::error::AppResult;
use crate::settings;

const PBKDF2_ROUNDS: u32 = 120_000;
const SALT_LEN: usize = 16;
const HASH_LEN: usize = 32;

#[derive(Default)]
struct GateState {
    /// 口令的哈希；None 表示这个部署不设防（只适合本机）
    code_hash: Option<String>,
    /// 令牌签名密钥。与口令分开存放，改口令时轮换它即可让旧会话全部失效。
    signing_key: Vec<u8>,
}

/// 访问控制：口令与签名密钥都放在这里，运行中可以整体替换，
/// 所以改口令不需要重启服务。
#[derive(Clone)]
pub struct AccessControl {
    state: Arc<RwLock<GateState>>,
    ttl_secs: i64,
}

impl AccessControl {
    /// 启动装配：优先沿用数据库里已有的口令；
    /// 数据库里还没有时，用环境变量做一次性初始化。
    pub async fn bootstrap(
        pool: &PgPool,
        env_code: Option<&str>,
        session_days: i64,
    ) -> anyhow::Result<Self> {
        let code_hash = match settings::get(pool, settings::ACCESS_CODE_HASH).await? {
            Some(stored) => Some(stored),
            None => match env_code {
                Some(code) => {
                    let encoded = encode_code(code);
                    settings::set(pool, settings::ACCESS_CODE_HASH, &encoded).await?;
                    tracing::info!("已用环境变量初始化访问口令，之后可在「设置」里修改");
                    Some(encoded)
                }
                None => None,
            },
        };

        let signing_key = match settings::get(pool, settings::SIGNING_KEY).await? {
            Some(hex) => decode_hex(&hex).unwrap_or_else(random_key),
            None => {
                let key = random_key();
                settings::set(pool, settings::SIGNING_KEY, &to_hex(&key)).await?;
                key
            }
        };

        Ok(Self {
            state: Arc::new(RwLock::new(GateState {
                code_hash,
                signing_key,
            })),
            ttl_secs: session_days * 86_400,
        })
    }

    pub fn is_protected(&self) -> bool {
        self.state
            .read()
            .map(|state| state.code_hash.is_some())
            .unwrap_or(false)
    }

    pub fn ttl_secs(&self) -> i64 {
        self.ttl_secs
    }

    pub fn verify_code(&self, code: &str) -> bool {
        let Ok(state) = self.state.read() else {
            return false;
        };
        match state.code_hash.as_deref() {
            Some(encoded) => verify_code(code, encoded),
            None => false,
        }
    }

    pub fn verify_token(&self, token: &str, now: i64) -> bool {
        let Ok(state) = self.state.read() else {
            return false;
        };
        state.code_hash.is_some() && token::verify(&state.signing_key, token, now)
    }

    pub fn issue_token(&self, now: i64) -> Option<String> {
        let state = self.state.read().ok()?;
        state.code_hash.as_ref()?;
        Some(token::issue(&state.signing_key, self.ttl_secs, now))
    }

    /// 改口令：写入新哈希并轮换签名密钥，所有旧登录立刻失效。
    pub async fn change_code(&self, pool: &PgPool, new_code: &str) -> AppResult<()> {
        let encoded = encode_code(new_code);
        let key = random_key();

        settings::set(pool, settings::ACCESS_CODE_HASH, &encoded).await?;
        settings::set(pool, settings::SIGNING_KEY, &to_hex(&key)).await?;

        {
            let mut state = self.state.write().unwrap_or_else(|err| err.into_inner());
            state.code_hash = Some(encoded);
            state.signing_key = key;
        }

        tracing::info!("访问口令已更新，旧会话全部失效");
        Ok(())
    }
}

// ---------------------------------------------------------------- 口令哈希

fn derive(code: &str, salt: &[u8], rounds: u32) -> [u8; HASH_LEN] {
    let mut out = [0u8; HASH_LEN];
    pbkdf2_hmac::<Sha256>(code.as_bytes(), salt, rounds, &mut out);
    out
}

fn encode_code(code: &str) -> String {
    let salt = Uuid::new_v4().into_bytes();
    let hash = derive(code, &salt, PBKDF2_ROUNDS);
    format!(
        "pbkdf2${PBKDF2_ROUNDS}${}${}",
        to_hex(&salt),
        to_hex(&hash)
    )
}

fn verify_code(code: &str, encoded: &str) -> bool {
    let parts: Vec<&str> = encoded.split('$').collect();
    if parts.len() != 4 || parts[0] != "pbkdf2" {
        return false;
    }
    let Ok(rounds) = parts[1].parse::<u32>() else {
        return false;
    };
    let Some(salt) = decode_hex(parts[2]) else {
        return false;
    };
    let Some(expected) = decode_hex(parts[3]) else {
        return false;
    };
    if salt.len() != SALT_LEN || expected.len() != HASH_LEN {
        return false;
    }

    let actual = derive(code, &salt, rounds);
    // 常量时间比较，避免通过响应耗时逐字节试出口令
    expected.ct_eq(&actual).into()
}

pub fn random_key() -> Vec<u8> {
    let mut key = Vec::with_capacity(32);
    key.extend_from_slice(Uuid::new_v4().as_bytes());
    key.extend_from_slice(Uuid::new_v4().as_bytes());
    key
}

pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn decode_hex(input: &str) -> Option<Vec<u8>> {
    if input.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();
    for chunk in bytes.chunks(2) {
        let text = std::str::from_utf8(chunk).ok()?;
        out.push(u8::from_str_radix(text, 16).ok()?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_hash_round_trip() {
        let encoded = encode_code("外婆家的鲤鱼");
        assert!(verify_code("外婆家的鲤鱼", &encoded));
        assert!(!verify_code("外婆家的鲤鱼 ", &encoded));
        assert!(!verify_code("别的口令", &encoded));
    }

    #[test]
    fn same_code_hashes_differently() {
        // 加了随机盐，两次编码不应相同
        let a = encode_code("同一个口令");
        let b = encode_code("同一个口令");
        assert_ne!(a, b);
        assert!(verify_code("同一个口令", &a));
        assert!(verify_code("同一个口令", &b));
    }

    #[test]
    fn malformed_hash_rejected() {
        assert!(!verify_code("x", ""));
        assert!(!verify_code("x", "plaintext"));
        assert!(!verify_code("x", "pbkdf2$abc$00$00"));
        assert!(!verify_code("x", "sha256$1$00$00"));
    }

    #[test]
    fn hex_round_trip() {
        let raw = random_key();
        assert_eq!(decode_hex(&to_hex(&raw)).unwrap(), raw);
        assert!(decode_hex("abc").is_none());
        assert!(decode_hex("zz").is_none());
    }
}

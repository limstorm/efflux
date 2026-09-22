//! 令牌桶限流。
//!
//! Docker 部署里这层活在 Nginx 上（`limit_req`），可本地部署没有 Nginx，端口一开就是裸的，
//! 所以后端自己也带一份：两边行为一致，Docker 里等于多一道保险。
//!
//! 两只桶：**登录**最紧（防猜口令），**其余 API** 一只宽松的。桶按客户端地址分，
//! 令牌按时间匀速回补，攒下的那点余量对应 Nginx 配置里的 `burst`。
//! `/files/` 不走这里 —— 图片是并着取的，卡这个只会让相册一顿一顿。

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::state::AppState;

/// 整站 API：30 请求/秒/IP，一口气能攒 60 次（与 Nginx 那边同值）
pub const API_PER_MINUTE: f64 = 30.0 * 60.0;
pub const API_BURST: f64 = 60.0;
/// 登录：12 次/分钟/IP，攒 5 次 —— 口令是家里的一句话，经不起一直猜
pub const LOGIN_PER_MINUTE: f64 = 12.0;
pub const LOGIN_BURST: f64 = 5.0;

/// 登录接口的路径：它只走自己那只紧桶，不再被 API 桶扣一次
const LOGIN_PATH: &str = "/api/auth/login";
/// 一个地址多久没动静就忘掉它（再来重新开始算）
const IDLE: Duration = Duration::from_secs(10 * 60);
/// 桶攒到这个数就顺手扫一遍，把凉掉的地址清出去
const SWEEP_AT: usize = 256;

struct Bucket {
    tokens: f64,
    last: Instant,
}

pub struct Limiter {
    /// 每秒回补多少令牌
    rate: f64,
    /// 桶的容量：一口气能攒下多少次
    burst: f64,
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
    ticks: AtomicU64,
}

impl Limiter {
    pub fn new(per_minute: f64, burst: f64) -> Self {
        Self {
            rate: per_minute / 60.0,
            burst,
            buckets: Mutex::new(HashMap::new()),
            ticks: AtomicU64::new(0),
        }
    }

    /// 整站 API 的那只桶
    pub fn api() -> Self {
        Self::new(API_PER_MINUTE, API_BURST)
    }

    /// 登录的那只桶
    pub fn login() -> Self {
        Self::new(LOGIN_PER_MINUTE, LOGIN_BURST)
    }

    /// 要一个令牌。`Err(等待秒数)` 表示这一下该拒。
    pub fn check(&self, ip: IpAddr) -> Result<(), u64> {
        let now = Instant::now();
        // 锁里只做算术，不做 IO；拿不到锁（有人 panic 过）就继续用，别把服务拖垮
        let mut buckets = self.buckets.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        if buckets.len() >= SWEEP_AT && self.ticks.fetch_add(1, Ordering::Relaxed) % 64 == 0 {
            buckets.retain(|_, bucket| now.duration_since(bucket.last) < IDLE);
        }

        let (rate, burst) = (self.rate, self.burst);
        let bucket = buckets
            .entry(ip)
            .or_insert_with(|| Bucket { tokens: burst, last: now });

        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * rate).min(burst);
        bucket.last = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            return Ok(());
        }

        // 还差多少才凑够一个令牌，向上取整成秒
        let wait = ((1.0 - bucket.tokens) / rate).ceil().max(1.0);
        Err(wait as u64)
    }
}

/// 请求来自哪：前面站着 Nginx / Caddy / Tunnel（对端是本机或内网）时，信 `X-Forwarded-For`
/// 里的头一条；直接暴露在公网时那个头谁都能编，只认 TCP 的对端地址。
fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<IpAddr> {
    let peer_ip = peer.map(|addr| addr.ip());
    if peer_ip.is_some_and(is_private) {
        if let Some(ip) = forwarded_ip(headers) {
            return Some(ip);
        }
    }
    peer_ip
}

/// 对端地址由 `into_make_service_with_connect_info` 塞进请求扩展里，
/// 就是 `ConnectInfo` 提取器读的那一格。
fn peer_addr(request: &Request) -> Option<SocketAddr> {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0)
}

fn forwarded_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let value = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))?;
    value
        .to_str()
        .ok()?
        .split(',')
        .next()?
        .trim()
        .parse()
        .ok()
}

fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4 == Ipv4Addr::UNSPECIFIED
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.is_loopback()
                || (first & 0xfe00) == 0xfc00   // fc00::/7 唯一的本地地址
                || (first & 0xffc0) == 0xfe80   // fe80::/10 链路本地
                || v6 == Ipv6Addr::UNSPECIFIED
        }
    }
}

fn too_many(retry_after: u64) -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::RETRY_AFTER, retry_after.to_string())],
        Json(json!({
            "error": {
                "code": "TOO_MANY_REQUESTS",
                "message": "慢一点，歇一口气再来"
            }
        })),
    )
        .into_response()
}

/// 整站 API 的宽松桶。登录有自己那只，不在这儿重复扣。
pub async fn guard_api(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !state.config.rate_limit || request.uri().path() == LOGIN_PATH {
        return next.run(request).await;
    }

    match client_ip(request.headers(), peer_addr(&request)) {
        Some(ip) => match state.api_limit.check(ip) {
            Ok(()) => next.run(request).await,
            Err(wait) => too_many(wait),
        },
        // 拿不到对端地址（理论上不会）：放过去，总比整个站点卡住强
        None => next.run(request).await,
    }
}

/// 登录的紧桶
pub async fn guard_login(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !state.config.rate_limit {
        return next.run(request).await;
    }

    match client_ip(request.headers(), peer_addr(&request)) {
        Some(ip) => match state.login_limit.check(ip) {
            Ok(()) => next.run(request).await,
            Err(wait) => too_many(wait),
        },
        None => next.run(request).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    /// 攒下的那点余量用完就该拒，且只拒这一个地址
    #[test]
    fn burst_runs_out_then_refuses() {
        let limiter = Limiter::login();
        for _ in 0..LOGIN_BURST as usize {
            assert!(limiter.check(ip("203.0.113.7")).is_ok());
        }
        assert!(limiter.check(ip("203.0.113.7")).is_err());
        // 换个地址不受影响
        assert!(limiter.check(ip("203.0.113.8")).is_ok());
    }

    /// 直连时 X-Forwarded-For 是编的，只认 TCP 的对端地址
    #[test]
    fn direct_request_uses_peer_address() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        let peer = Some("198.51.100.9:5555".parse().unwrap());
        // 对端是公网地址：X-Forwarded-For 是编的，不认
        assert_eq!(client_ip(&headers, peer), Some(ip("198.51.100.9")));
    }

    /// 前面站着代理（对端是内网）时，按转发头里的头一条算
    #[test]
    fn proxy_request_uses_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 10.0.0.1".parse().unwrap());
        let peer = Some("127.0.0.1:5555".parse().unwrap());
        assert_eq!(client_ip(&headers, peer), Some(ip("1.2.3.4")));

        // 没有转发头就退回对端地址
        assert_eq!(client_ip(&HeaderMap::new(), peer), Some(ip("127.0.0.1")));
    }
}

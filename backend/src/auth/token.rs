use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// 会话令牌：`过期时间戳.HMAC 签名`。
///
/// 令牌本身不含任何身份信息，服务端也不需要会话表——
/// 只要签名密钥不变，签名就一直有效；轮换密钥（改口令时会做）等于踢掉所有旧会话。
pub fn issue(key: &[u8], ttl_secs: i64, now: i64) -> String {
    let expires = now + ttl_secs;
    let payload = expires.to_string();
    let signature = sign(key, &payload);
    format!("{payload}.{signature}")
}

pub fn verify(key: &[u8], token: &str, now: i64) -> bool {
    let Some((payload, signature)) = token.split_once('.') else {
        return false;
    };
    let Ok(expires) = payload.parse::<i64>() else {
        return false;
    };
    if expires <= now {
        return false;
    }

    let expected = sign(key, payload);
    // 常量时间比较，避免通过响应耗时逐字节猜签名
    expected.as_bytes().ct_eq(signature.as_bytes()).into()
}

fn sign(key: &[u8], payload: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC 接受任意长度密钥");
    mac.update(payload.as_bytes());
    let bytes = mac.finalize().into_bytes();

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"a-test-signing-key";
    const NOW: i64 = 1_700_000_000;

    #[test]
    fn token_round_trip() {
        let token = issue(KEY, 30 * 86_400, NOW);
        assert!(verify(KEY, &token, NOW));
        assert!(verify(KEY, &token, NOW + 29 * 86_400));
        assert!(!verify(KEY, &token, NOW + 31 * 86_400));
    }

    #[test]
    fn tampered_token_rejected() {
        let token = issue(KEY, 30 * 86_400, NOW);
        let (payload, _) = token.split_once('.').unwrap();

        // 自己把过期时间往后改
        let forged = format!("{}.{}", payload.parse::<i64>().unwrap() + 86_400, "00");
        assert!(!verify(KEY, &forged, NOW));

        // 换个密钥签的令牌
        assert!(!verify(b"another-key", &token, NOW));

        // 格式不对的
        assert!(!verify(KEY, "not-a-token", NOW));
        assert!(!verify(KEY, "abc.def", NOW));
    }
}

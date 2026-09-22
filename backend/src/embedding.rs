//! 向量的存取与比较。
//!
//! CLIP 出来的向量在 `encode_image` 里已经 L2 归一化过，所以余弦相似度就是
//! 点积，不用再算模长。存库时摊成 f32 小端字节，读回来照原样拼上。
//!
//! 没有装 pgvector：几万张图全量读出来在内存里比一遍也就几十毫秒，不值得
//! 为它多一个部署依赖（扩展通常还得超级用户才装得上）。

/// 把向量摊成字节（f32 小端）
pub fn to_bytes(vec: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vec.len() * 4);
    for value in vec {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// 把字节拼回向量。长度不是 4 的倍数说明这段数据坏了，返回 None
pub fn from_bytes(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

/// 余弦相似度。两边都已归一化，点积就是余弦。
/// 长度对不上说明不是同一个模型算的，返回 None 而不是硬算。
pub fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    Some(a.iter().zip(b).map(|(x, y)| x * y).sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_bytes() {
        let vec = vec![0.5f32, -1.25, 3.0];
        let bytes = to_bytes(&vec);
        assert_eq!(bytes.len(), 12);
        assert_eq!(from_bytes(&bytes).unwrap(), vec);
        // 坏数据不该被当成向量
        assert!(from_bytes(&[1, 2, 3]).is_none());
        assert!(from_bytes(&[]).is_none());
    }

    #[test]
    fn cosine_of_normalized_vectors() {
        let a = vec![1.0f32, 0.0];
        let same = vec![1.0f32, 0.0];
        let orthogonal = vec![0.0f32, 1.0];
        assert!((cosine(&a, &same).unwrap() - 1.0).abs() < 1e-6);
        assert!(cosine(&a, &orthogonal).unwrap().abs() < 1e-6);
        // 维度不一致就不该比
        assert!(cosine(&a, &[1.0]).is_none());
    }
}

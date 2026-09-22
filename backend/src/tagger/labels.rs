//! 标签集：编译期嵌进二进制的 labels.json。
//!
//! 每个标签带一条英文 prompt —— CLIP 的文本塔是英文训练的，喂中文会掉点。
//! 显示用的是中文 name，两者在 labels.json 里成对维护。

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct LabelDef {
    pub key: String,
    pub name: String,
    pub group: String,
    /// 英文提示词，直接喂 CLIP
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupDef {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct Raw {
    groups: Vec<GroupDef>,
    labels: Vec<LabelDef>,
}

pub struct Labels {
    pub defs: Vec<LabelDef>,
    pub groups: Vec<GroupDef>,
    group_name_of: HashMap<String, String>,
}

impl Labels {
    pub fn load() -> AppResult<Arc<Self>> {
        let raw: Raw = serde_json::from_str(include_str!("labels.json"))
            .map_err(|err| AppError::Internal(anyhow::anyhow!("标签集解析失败：{err}")))?;

        if raw.labels.is_empty() {
            return Err(AppError::Internal(anyhow::anyhow!("标签集是空的")));
        }

        let by_key: HashMap<&str, &str> = raw
            .groups
            .iter()
            .map(|g| (g.key.as_str(), g.name.as_str()))
            .collect();

        let group_name_of = raw
            .labels
            .iter()
            .map(|label| {
                let name = by_key
                    .get(label.group.as_str())
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| label.group.clone());
                (label.key.clone(), name)
            })
            .collect();

        Ok(Arc::new(Self {
            defs: raw.labels,
            groups: raw.groups,
            group_name_of,
        }))
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// 标签所属分类的中文名
    pub fn group_name(&self, label_key: &str) -> String {
        self.group_name_of
            .get(label_key)
            .cloned()
            .unwrap_or_else(|| "其他".to_string())
    }
}

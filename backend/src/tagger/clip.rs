//! CLIP 推理：图片编码 + 标签打分。
//!
//! 文本塔只在启动时跑一次（300 条 prompt），之后每张图只走视觉塔，
//! 于是单张的耗时基本就是一次 224×224 的前向。
//!
//! 注意 `Session::run` 要 `&mut self`，所以外面用 `Mutex` 包一层共享；
//! 推理本身是同步 CPU 活，调用方记得放进 spawn_blocking。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::Tokenizer;

use super::labels::Labels;
use crate::error::{AppError, AppResult};

const IMAGE_SIZE: usize = 224;
/// CLIP 的固定上下文长度，短了要补 pad
const TEXT_LEN: usize = 77;

/// CLIP 官方的预处理均值 / 标准差
const MEAN: [f32; 3] = [0.481_454_66, 0.457_827_5, 0.408_210_73];
const STD: [f32; 3] = [0.268_629_54, 0.261_302_58, 0.275_777_11];

/// 每组最多留几个，免得一张图全是"风景"类标签
const PER_GROUP_MAX: usize = 2;
/// 先攒这么多候选，再全局排序
const CANDIDATE_POOL: usize = 24;
/// 最终最多保留几个标签
const MAX_TAGS: usize = 8;
/// 绝对下限（未乘 logit_scale 的余弦相似度）
const MIN_SCORE: f32 = 0.15;
/// 相对下限：不足最高分的这个比例就丢掉
const RELATIVE_KEEP: f32 = 0.86;

#[derive(Debug, Clone)]
pub struct TagHit {
    pub key: String,
    pub name: String,
    pub group: String,
    pub score: f32,
}

pub struct Tagger {
    vision: Session,
    text: Session,
    tokenizer: Tokenizer,
    labels: Arc<Labels>,
    /// 300 条标签 prompt 的文本向量，启动时算好
    text_embeds: Vec<Vec<f32>>,
    vision_input: String,
    text_input_ids: String,
    /// 有的导出脚本（比如 Xenova 这版）只接受 input_ids，没有 mask 这个输入
    text_input_mask: Option<String>,
}

impl Tagger {
    pub fn load(model_dir: &Path, labels: Arc<Labels>) -> AppResult<Self> {
        let vision_path = model_dir.join("vision_model.onnx");
        let text_path = model_dir.join("text_model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        for path in [&vision_path, &text_path, &tokenizer_path] {
            if !path.exists() {
                return Err(AppError::Internal(anyhow::anyhow!(
                    "缺少模型文件 {}（模型不随仓库分发，见 README 的说明）",
                    path.display()
                )));
            }
        }

        let vision = load_session(&vision_path, "视觉")?;
        let text = load_session(&text_path, "文本")?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|err| AppError::Internal(anyhow::anyhow!("加载分词器失败：{err}")))?;

        // 输入名各版本导出可能不同，取模型自己声明的那个，别硬编码
        let vision_input = vision
            .inputs()
            .first()
            .map(|input| input.name().to_string())
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("视觉模型没有输入")))?;
        let text_input_ids = text
            .inputs()
            .first()
            .map(|input| input.name().to_string())
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("文本模型没有输入")))?;
        // 有的导出（比如 Xenova 这版）只接受 input_ids，别硬塞一个不存在的名字
        let text_input_mask = text.inputs().get(1).map(|input| input.name().to_string());

        tracing::info!(
            %vision_input,
            %text_input_ids,
            ?text_input_mask,
            labels = labels.len(),
            "CLIP 模型已载入"
        );

        let mut tagger = Self {
            vision,
            text,
            tokenizer,
            labels,
            text_embeds: Vec::new(),
            vision_input,
            text_input_ids,
            text_input_mask,
        };

        let started = std::time::Instant::now();
        let text_embeds = tagger.encode_all_labels()?;
        tagger.text_embeds = text_embeds;
        tracing::info!(
            count = tagger.labels.len(),
            ms = started.elapsed().as_millis(),
            "标签向量已备好"
        );

        Ok(tagger)
    }

    pub fn label_count(&self) -> usize {
        self.labels.len()
    }

    fn encode_all_labels(&mut self) -> AppResult<Vec<Vec<f32>>> {
        // 先把 prompt 摘出来：一边借 labels 一边又要 &mut self，借用检查器不让过
        let prompts: Vec<String> = self
            .labels
            .defs
            .iter()
            .map(|def| def.prompt.clone())
            .collect();

        let mut out = Vec::with_capacity(prompts.len());
        for prompt in &prompts {
            out.push(self.encode_text(prompt)?);
        }
        Ok(out)
    }

    fn encode_text(&mut self, text: &str) -> AppResult<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|err| AppError::Internal(anyhow::anyhow!("分词失败：{err}")))?;

        let ids_src = encoding.get_ids();
        let mask_src = encoding.get_attention_mask();
        let take = ids_src.len().min(TEXT_LEN);

        let mut ids = vec![0i64; TEXT_LEN];
        let mut mask = vec![0i64; TEXT_LEN];
        for i in 0..take {
            ids[i] = ids_src[i] as i64;
            mask[i] = mask_src[i] as i64;
        }

        let ids_tensor = Tensor::from_array(([1usize, TEXT_LEN], ids))
            .map_err(|err| AppError::Internal(anyhow::anyhow!("构造张量失败：{err}")))?;

        let outputs = match self.text_input_mask.clone() {
            Some(mask_name) => {
                let mask_tensor = Tensor::from_array(([1usize, TEXT_LEN], mask))
                    .map_err(|err| AppError::Internal(anyhow::anyhow!("构造张量失败：{err}")))?;
                self.text.run(ort::inputs![
                    self.text_input_ids.as_str() => ids_tensor,
                    mask_name.as_str() => mask_tensor,
                ])
            }
            None => self
                .text
                .run(ort::inputs![self.text_input_ids.as_str() => ids_tensor]),
        }
        .map_err(|err| AppError::Internal(anyhow::anyhow!("文本推理失败：{err}")))?;

        let mut embed = first_embedding(&outputs)?;
        l2_normalize(&mut embed);
        Ok(embed)
    }

    fn encode_image(&mut self, path: &Path) -> AppResult<Vec<f32>> {
        let pixels = read_image_chw(path)?;
        let tensor = Tensor::from_array(([1usize, 3, IMAGE_SIZE, IMAGE_SIZE], pixels))
            .map_err(|err| AppError::Internal(anyhow::anyhow!("构造张量失败：{err}")))?;

        let outputs = self
            .vision
            .run(ort::inputs![self.vision_input.as_str() => tensor])
            .map_err(|err| AppError::Internal(anyhow::anyhow!("视觉推理失败：{err}")))?;

        let mut embed = first_embedding(&outputs)?;
        l2_normalize(&mut embed);
        Ok(embed)
    }

    /// 给一张图打分，返回挑好的标签（已按分数降序）与这张图的向量。
    ///
    /// 向量本来算完就扔，现在一并交出去存下来——同一张图以后再要和别的照片
    /// 比距离，就不必重新过一遍模型了。
    pub fn tag_image(&mut self, path: &Path) -> AppResult<(Vec<TagHit>, Vec<f32>)> {
        let embed = self.encode_image(path)?;

        let mut hits: Vec<TagHit> = self
            .labels
            .defs
            .iter()
            .zip(self.text_embeds.iter())
            .map(|(def, text_embed)| TagHit {
                key: def.key.clone(),
                name: def.name.clone(),
                group: self.labels.group_name(&def.key),
                score: dot(&embed, text_embed),
            })
            .collect();

        hits.sort_by(|a, b| b.score.total_cmp(&a.score));

        // 先按"每组最多两个"攒候选，标签才有跨度，不会清一色风景
        let mut used_per_group: HashMap<&str, usize> = HashMap::new();
        let mut pool: Vec<TagHit> = Vec::with_capacity(CANDIDATE_POOL);
        for hit in &hits {
            let used = used_per_group.entry(hit.group.as_str()).or_insert(0);
            if *used >= PER_GROUP_MAX {
                continue;
            }
            *used += 1;
            pool.push(hit.clone());
            if pool.len() >= CANDIDATE_POOL {
                break;
            }
        }

        let top = pool.first().map(|hit| hit.score).unwrap_or(0.0);
        let floor = (top * RELATIVE_KEEP).max(MIN_SCORE);

        let mut chosen: Vec<TagHit> = pool
            .into_iter()
            .filter(|hit| hit.score >= floor)
            .take(MAX_TAGS)
            .collect();

        if chosen.is_empty() {
            // 全被阈值刷掉时至少留最高分那个，别让图集里出现"无标签"的空档
            if let Some(best) = hits.into_iter().next() {
                chosen.push(best);
            }
        }

        Ok((chosen, embed))
    }
}

fn load_session(path: &Path, what: &str) -> AppResult<Session> {
    Session::builder()
        .map_err(|err| AppError::Internal(anyhow::anyhow!("初始化 ONNX Runtime 失败：{err}")))?
        .commit_from_file(path)
        .map_err(|err| AppError::Internal(anyhow::anyhow!("加载{what}模型失败：{err}")))
}

/// 取出第一个输出的第一行，当作 embedding
fn first_embedding(outputs: &ort::session::SessionOutputs<'_>) -> AppResult<Vec<f32>> {
    // 按位置取更稳：不同导出脚本的输出名不一致
    let (_, value) = outputs
        .iter()
        .next()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("模型没有输出")))?;

    let (shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|err| AppError::Internal(anyhow::anyhow!("读取模型输出失败：{err}")))?;

    // [1, dim] 或 [1, seq, dim] 都取第一段
    let dim = shape.last().copied().unwrap_or(0) as usize;
    if dim == 0 || data.len() < dim {
        return Err(AppError::Internal(anyhow::anyhow!(
            "模型输出形状异常：{shape:?}"
        )));
    }
    Ok(data[..dim].to_vec())
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn l2_normalize(values: &mut [f32]) {
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 1e-6 {
        for value in values.iter_mut() {
            *value /= norm;
        }
    }
}

/// 解码 → 缩放 → 归一化 → NCHW
fn read_image_chw(path: &Path) -> AppResult<Vec<f32>> {
    let image = image::open(path)
        .map_err(|err| AppError::Internal(anyhow::anyhow!("解码图片失败：{err}")))?;

    // CatmullRom 就是双三次，和 CLIP 官方预处理的 bicubic 对得上
    let resized = image.resize_exact(
        IMAGE_SIZE as u32,
        IMAGE_SIZE as u32,
        image::imageops::FilterType::CatmullRom,
    );
    let rgb = resized.to_rgb8();

    let plane = IMAGE_SIZE * IMAGE_SIZE;
    let mut out = vec![0f32; 3 * plane];
    for (x, y, pixel) in rgb.enumerate_pixels() {
        let index = (y as usize) * IMAGE_SIZE + (x as usize);
        for channel in 0..3 {
            let value = pixel[channel] as f32 / 255.0;
            out[channel * plane + index] = (value - MEAN[channel]) / STD[channel];
        }
    }
    Ok(out)
}

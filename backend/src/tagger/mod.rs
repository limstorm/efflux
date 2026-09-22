//! 图片自动打标签：用 CLIP 做零样本分类，标签集见 labels.json。
//!
//! 整块功能由 EFFLUX_TAGGER 控制：不开就是普通的上传流程，一点额外开销都没有。

mod clip;
mod labels;
mod pipeline;

pub use clip::Tagger;
pub use labels::Labels;
pub use pipeline::spawn as spawn_tagger;

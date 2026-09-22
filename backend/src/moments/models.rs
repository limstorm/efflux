use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------- 数据库行

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MomentRow {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub happened_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub location_name: String,
    pub city: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// 氛围音的名字（rain / night / fire / pad）。合成出来的，没有文件
    pub ambient: Option<String>,
    /// 分享凭证。没分享过就是 None
    pub share_token: Option<String>,
}

/// 发生地点。
///
/// `city` 是地图点亮的粒度——一盏灯代表一座城。定位只给得出经纬度，
/// 落不到城市上就没法聚合，所以坐标一进来就顺手推断出最近的市。
/// 名字、城市、坐标都可以缺，但至少要有其一，这条记忆才算「有地点」。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub city: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

impl Place {
    /// 名字、城市、坐标全都没有，就算没填地点
    pub fn from_row(row: &MomentRow) -> Option<Self> {
        let name = row.location_name.trim();
        let city = row.city.trim();
        if name.is_empty() && city.is_empty() && row.latitude.is_none() {
            return None;
        }
        Some(Self {
            name: name.to_string(),
            city: city.to_string(),
            latitude: row.latitude,
            longitude: row.longitude,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MediaRow {
    pub id: Uuid,
    pub moment_id: Option<Uuid>,
    pub kind: String,
    pub role: String,
    pub file_path: String,
    pub thumb_path: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
    pub position: i32,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagWithCount {
    pub name: String,
    pub count: i64,
}

// ---------------------------------------------------------------- 输出 DTO

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDto {
    pub id: Uuid,
    pub kind: String,
    pub role: String,
    pub url: String,
    pub thumb_url: Option<String>,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
    pub size_bytes: i64,
    pub position: i32,
}

impl From<MediaRow> for MediaDto {
    fn from(row: MediaRow) -> Self {
        let url = file_url(&row.file_path);
        let thumb_url = row.thumb_path.as_deref().map(file_url);
        Self {
            id: row.id,
            kind: row.kind,
            role: row.role,
            url,
            thumb_url,
            mime_type: row.mime_type,
            width: row.width,
            height: row.height,
            duration_ms: row.duration_ms,
            size_bytes: row.size_bytes,
            position: row.position,
        }
    }
}

fn file_url(path: &str) -> String {
    format!("/files/{}", path.trim_start_matches('/'))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentSummary {
    pub id: Uuid,
    pub title: String,
    pub excerpt: String,
    pub happened_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
    /// 时间轴上的展示封面（第一张图 / 视频缩略图）
    pub cover: Option<MediaDto>,
    pub media_count: usize,
    pub has_music: bool,
    pub place: Option<Place>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentDetail {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub happened_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub media: Vec<MediaDto>,
    pub music: Option<MediaDto>,
    /// 氛围音（没有音乐文件时的默认气氛）。与 music 二选一
    pub ambient: Option<String>,
    pub place: Option<Place>,
    /// 这条记忆的分享凭证；null 表示还没分享出去。带上它才拼得出分享链接
    pub share_token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentListResponse {
    pub items: Vec<MomentSummary>,
    pub total: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsResponse {
    pub total: i64,
    pub first_happened_at: Option<DateTime<Utc>>,
    pub last_happened_at: Option<DateTime<Utc>>,
    pub tag_count: i64,
}

// ---------------------------------------------------------------- 输入 DTO

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaLink {
    pub id: Uuid,
    #[serde(default)]
    pub position: Option<i32>,
}

/// 创建与更新共用同一套字段（更新时全部可选，None 表示不动）。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentInput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub happened_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub media: Option<Vec<MediaLink>>,
    /// 氛围音的名字：rain / night / fire / pad。
    /// 不给就不动；给空串表示不要了；给不认识的名字会被拒
    #[serde(default)]
    pub ambient: Option<String>,
    /// 发生地点。三个字段都是可选的：可以只写个名字，
    /// 也可以只带坐标（浏览器定位），也可以都填。
    #[serde(default)]
    pub location_name: Option<String>,
    /// 市。留空时只要带了坐标，后端会自动推断一个最近的市
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    /// 关键词。标题、正文、地点名、城市都在它的射程内
    pub q: Option<String>,
    /// 逗号分隔的标签名
    pub tags: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub order: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

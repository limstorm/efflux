//! 点亮地图：把记忆落到行政区划上。
//!
//! 两件事：把「我存的城市名」翻成「官方行政区划名」（前端拿后者去和地图多边形对），
//! 以及按城市聚合出记忆条数。
//!
//! 名字这件事非做不可：我的表里存的是「康定」，而地图上是「甘孜藏族自治州」；
//! 不做这层翻译，西藏、云南、新疆、贵州那一大片会全黑——明明去过，却没有一盏灯亮。
//!
//! 翻译用的两张表是离线生成的（见 `city-map.json` 与 `provinces.json`，
//! 由 DataV 的行政区划数据整理而来），编译进二进制，运行时不需要联网。

use std::collections::HashMap;
use std::sync::OnceLock;

use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::AppResult;
use crate::state::AppState;

/// `[我存的城市名, 官方行政区划名, 市 adcode, 省 adcode]`
const CITY_MAP: &str = include_str!("map/city-map.json");
/// `[省 adcode, 省名]`
const PROVINCES: &str = include_str!("map/provinces.json");

#[derive(Debug, Clone)]
struct CityEntry {
    /// 地图多边形上的名字，比如「杭州市」
    official: String,
    adcode: u32,
    province: u32,
}

fn city_index() -> &'static HashMap<String, CityEntry> {
    static INDEX: OnceLock<HashMap<String, CityEntry>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let rows: Vec<(String, String, u32, u32)> =
            serde_json::from_str(CITY_MAP).expect("city-map.json 应当合法");
        rows.into_iter()
            .map(|(city, official, adcode, province)| {
                (
                    city,
                    CityEntry {
                        official,
                        adcode,
                        province,
                    },
                )
            })
            .collect()
    })
}

fn province_names() -> &'static HashMap<u32, String> {
    static NAMES: OnceLock<HashMap<u32, String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let rows: Vec<(u32, String)> =
            serde_json::from_str(PROVINCES).expect("provinces.json 应当合法");
        rows.into_iter().collect()
    })
}

// ---------------------------------------------------------------- 查询

#[derive(Debug, sqlx::FromRow)]
struct CityVisits {
    city: String,
    visits: i64,
    first_at: Option<DateTime<Utc>>,
    last_at: Option<DateTime<Utc>>,
}

/// 按城市聚合。命中 moments_city_idx，几千条记忆也就几毫秒
async fn visits_by_city(pool: &PgPool) -> AppResult<Vec<CityVisits>> {
    let rows = sqlx::query_as::<_, CityVisits>(
        "SELECT city, count(*) AS visits, \
                min(happened_at) AS first_at, max(happened_at) AS last_at \
         FROM moments WHERE city <> '' GROUP BY city",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ---------------------------------------------------------------- 省级

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvinceLit {
    pub name: String,
    pub adcode: u32,
    /// 去过的城市数
    pub cities: usize,
    pub visits: i64,
    pub last_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvinceMapResponse {
    pub provinces: Vec<ProvinceLit>,
    pub total_provinces: usize,
    pub total_cities: usize,
    pub total_visits: i64,
    /// 海外城市：地图上放不下，卡片里单独说一句
    pub overseas: Vec<String>,
    /// 既不在映射里、也不像国外的城市——理论上为 0
    pub elsewhere: usize,
}

/// `GET /api/map/provinces`
pub async fn provinces(State(state): State<AppState>) -> AppResult<Json<ProvinceMapResponse>> {
    let rows = visits_by_city(&state.pool).await?;
    let index = city_index();
    let names = province_names();

    let mut buckets: HashMap<u32, ProvinceLit> = HashMap::new();
    let mut total_cities = 0usize;
    let mut total_visits = 0i64;
    let mut overseas: Vec<String> = Vec::new();
    let mut elsewhere = 0usize;

    for row in rows {
        let entry = match index.get(&row.city) {
            Some(entry) => entry,
            None => {
                // 不在表里：多半是海外城市（东京、巴黎这些我特意留着的）
                if looks_overseas(&row.city) {
                    overseas.push(row.city);
                } else {
                    elsewhere += 1;
                    tracing::debug!(city = %row.city, "这个城市没在地图映射里");
                }
                continue;
            }
        };

        total_cities += 1;
        total_visits += row.visits;

        let slot = buckets.entry(entry.province).or_insert_with(|| ProvinceLit {
            name: names
                .get(&entry.province)
                .cloned()
                .unwrap_or_else(|| format!("{}", entry.province)),
            adcode: entry.province,
            cities: 0,
            visits: 0,
            last_at: None,
        });
        slot.cities += 1;
        slot.visits += row.visits;
        // 取最近的一次
        if let Some(last) = row.last_at {
            slot.last_at = Some(match slot.last_at {
                Some(current) if current > last => current,
                _ => last,
            });
        }
    }

    let mut provinces: Vec<ProvinceLit> = buckets.into_values().collect();
    // 点得最亮的排前面
    provinces.sort_by(|a, b| b.visits.cmp(&a.visits).then(a.name.cmp(&b.name)));
    overseas.sort();

    Ok(Json(ProvinceMapResponse {
        total_provinces: provinces.len(),
        provinces,
        total_cities,
        total_visits,
        overseas,
        elsewhere,
    }))
}

/// 粗略判断是不是海外城市。
///
/// 表里的海外城市都是常见的中文译名（东京、巴黎、纽约…），而国内那 28 个没补上
/// 的地方全是「XX市 / XX县 / XX林区」这类带行政后缀的名字。用后缀区分就够了，
/// 不必单独维护一张清单。
fn looks_overseas(city: &str) -> bool {
    const CN_SUFFIX: &[&str] = &["市", "县", "区", "旗", "林区", "地区"];
    !CN_SUFFIX.iter().any(|suffix| city.ends_with(suffix))
}

// ---------------------------------------------------------------- 市级

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CityLit {
    /// 我存的名字（「杭州」）
    pub name: String,
    /// 官方名（「杭州市」）——前端拿它和地图多边形对上
    pub map_name: String,
    pub adcode: u32,
    pub visits: i64,
    pub first_at: Option<DateTime<Utc>>,
    pub last_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CityMapQuery {
    pub province: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CityMapResponse {
    pub province: u32,
    pub name: String,
    pub cities: Vec<CityLit>,
}

/// `GET /api/map/cities?province=330000`
pub async fn cities(
    State(state): State<AppState>,
    Query(query): Query<CityMapQuery>,
) -> AppResult<Json<CityMapResponse>> {
    let rows = visits_by_city(&state.pool).await?;
    let index = city_index();

    let mut cities: Vec<CityLit> = Vec::new();
    for row in rows {
        let Some(entry) = index.get(&row.city) else {
            continue;
        };
        if entry.province != query.province {
            continue;
        }
        cities.push(CityLit {
            name: row.city,
            map_name: entry.official.clone(),
            adcode: entry.adcode,
            visits: row.visits,
            first_at: row.first_at,
            last_at: row.last_at,
        });
    }

    cities.sort_by(|a, b| b.visits.cmp(&a.visits).then(a.name.cmp(&b.name)));

    let name = province_names()
        .get(&query.province)
        .cloned()
        .unwrap_or_else(|| query.province.to_string());

    Ok(Json(CityMapResponse {
        province: query.province,
        name,
        cities,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_tables_load() {
        let index = city_index();
        let names = province_names();
        assert!(index.len() > 300, "城市映射应当有三百多条");
        assert_eq!(names.len(), 34, "省级单位应有 34 个");

        // 几个容易错的
        assert_eq!(index.get("杭州").unwrap().official, "杭州市");
        assert_eq!(index.get("康定").unwrap().official, "甘孜藏族自治州");
        assert_eq!(index.get("锡林浩特").unwrap().official, "锡林郭勒盟");
        assert_eq!(index.get("加格达奇").unwrap().official, "大兴安岭地区");
        assert_eq!(index.get("北京").unwrap().adcode, 110000);
    }

    #[test]
    fn overseas_heuristic() {
        assert!(looks_overseas("东京"));
        assert!(looks_overseas("巴黎"));
        // 国内那 28 个没补的地方都被排除在外
        assert!(!looks_overseas("仙桃市"));
        assert!(!looks_overseas("陵水黎族自治县"));
    }
}

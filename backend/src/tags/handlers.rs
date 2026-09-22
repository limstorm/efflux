use axum::extract::State;
use axum::Json;

use crate::error::AppResult;
use crate::moments::models::TagWithCount;
use crate::moments::repo;
use crate::state::AppState;

/// `GET /api/tags` — 全部标签及其使用次数，供搜索与创建时推荐。
pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<TagWithCount>>> {
    let tags = repo::all_tags(&state.pool).await?;
    Ok(Json(tags))
}

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use topcoat::context::{Cx, app_context};
use topcoat::router::content::Json;
use topcoat::router::route;
use topcoat::{Error, Result};

use super::super::require_internal_api;
use crate::database::{Database, LocalUser};

topcoat::router::segment!(rename = "fuzzy_search");

#[derive(Deserialize)]
struct FuzzySearchUsersBody {
    query: String,
    limit: u32,
}

#[derive(Serialize)]
struct UsersResponse {
    users: Vec<LocalUser>,
}

#[route(POST)]
async fn fuzzy_search_users(
    cx: &Cx,
    Json(body): Json<FuzzySearchUsersBody>,
) -> Result<Json<UsersResponse>> {
    require_internal_api(cx)?;
    let users = app_context::<Database>(cx)
        .fuzzy_search_users(&body.query, i64::from(body.limit.min(100)))
        .await
        .context("failed to search users")
        .map_err(Error::from_anyhow)?;
    Ok(Json(UsersResponse { users }))
}

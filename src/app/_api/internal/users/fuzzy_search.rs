use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{content::Json, error::internal_server_error, route},
};

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
        .map_err(internal_server_error)?;
    Ok(Json(UsersResponse { users }))
}

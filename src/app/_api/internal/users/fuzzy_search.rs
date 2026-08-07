use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, error::internal_server_error, headers, route},
};

use super::super::validate_internal_api_key;
use crate::{
    app::production_state,
    store::{self, LocalUser},
};

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
    let state = production_state(cx)?;
    validate_internal_api_key(state, headers(cx))?;
    let users =
        store::fuzzy_search_users(&state.database, &body.query, i64::from(body.limit.min(100)))
            .await
            .map_err(|error| internal("failed to search users", error))?;
    Ok(Json(UsersResponse { users }))
}

fn internal(context: &'static str, error: impl Into<anyhow::Error>) -> topcoat::Error {
    let error = error.into().context(context);
    tracing::error!(%error, "request failed");
    internal_server_error(error).into()
}

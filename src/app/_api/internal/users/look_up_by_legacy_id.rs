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

topcoat::router::segment!(rename = "look_up_by_legacy_id");

#[derive(Deserialize)]
struct LookUpUsersByLegacyIdBody {
    legacy_user_ids: Vec<i32>,
}

#[derive(Serialize)]
struct UsersResponse {
    users: Vec<LocalUser>,
}

#[route(POST)]
async fn look_up_users_by_legacy_id(
    cx: &Cx,
    Json(body): Json<LookUpUsersByLegacyIdBody>,
) -> Result<Json<UsersResponse>> {
    let state = production_state(cx)?;
    validate_internal_api_key(state, headers(cx))?;
    let users = store::users_by_ids(&state.database, &body.legacy_user_ids)
        .await
        .map_err(|error| internal("failed to look up users", error))?;
    Ok(Json(UsersResponse { users }))
}

fn internal(context: &'static str, error: impl Into<anyhow::Error>) -> topcoat::Error {
    let error = error.into().context(context);
    tracing::error!(%error, "request failed");
    internal_server_error(error).into()
}

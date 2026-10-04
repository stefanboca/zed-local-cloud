use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use topcoat::context::{Cx, app_context};
use topcoat::router::content::Json;
use topcoat::router::route;
use topcoat::{Error, Result};

use super::super::require_internal_api;
use crate::database::{Database, LocalUser};

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
    require_internal_api(cx)?;
    let users = app_context::<Database>(cx)
        .users_by_ids(&body.legacy_user_ids)
        .await
        .context("failed to look up users")
        .map_err(Error::from_anyhow)?;
    Ok(Json(UsersResponse { users }))
}

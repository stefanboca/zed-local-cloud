use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use topcoat::context::{Cx, app_context};
use topcoat::router::content::Json;
use topcoat::router::route;
use topcoat::{Error, Result};

use super::super::require_internal_api;
use crate::database::{Database, LocalUser};

topcoat::router::segment!(rename = "look_up_by_github_login");

#[derive(Deserialize)]
struct LookUpUserByGithubLoginBody {
    github_login: String,
}

#[derive(Serialize)]
struct UserResponse {
    user: Option<LocalUser>,
}

#[route(POST)]
async fn look_up_user_by_github_login(
    cx: &Cx,
    Json(body): Json<LookUpUserByGithubLoginBody>,
) -> Result<Json<UserResponse>> {
    require_internal_api(cx)?;
    let user = app_context::<Database>(cx)
        .user_by_github_login(&body.github_login)
        .await
        .context("failed to look up a user")
        .map_err(Error::from_anyhow)?;
    Ok(Json(UserResponse { user }))
}

use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{content::Json, error::internal_server_error, route},
};

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
        .map_err(|error| internal("failed to look up a user", error))?;
    Ok(Json(UserResponse { user }))
}

fn internal(context: &'static str, error: impl Into<anyhow::Error>) -> topcoat::Error {
    let error = error.into().context(context);
    tracing::error!(%error, "request failed");
    internal_server_error(error).into()
}

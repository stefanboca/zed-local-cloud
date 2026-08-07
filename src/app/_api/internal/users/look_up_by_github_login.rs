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
    let state = production_state(cx)?;
    validate_internal_api_key(state, headers(cx))?;
    let user = store::user_by_github_login(&state.database, &body.github_login)
        .await
        .map_err(|error| internal("failed to look up a user", error))?;
    Ok(Json(UserResponse { user }))
}

fn internal(context: &'static str, error: impl Into<anyhow::Error>) -> topcoat::Error {
    let error = error.into().context(context);
    tracing::error!(%error, "request failed");
    internal_server_error(error).into()
}

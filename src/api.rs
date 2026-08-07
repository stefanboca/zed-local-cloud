use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        HeaderMap, Response, StatusCode,
        content::{
            Json,
            websocket::{Message, WebSocket, WebSocketUpgrade},
        },
        error::{internal_server_error, unauthorized},
        header, headers, route,
    },
};

use crate::{
    app::AppState,
    store::{self, LocalUser},
};

#[derive(Debug)]
pub struct Credentials {
    pub user_id: i32,
    pub access_token: String,
}

#[derive(Deserialize)]
pub struct LookUpUsersByLegacyIdBody {
    legacy_user_ids: Vec<i32>,
}

#[derive(Serialize)]
pub struct UsersResponse {
    users: Vec<LocalUser>,
}

#[derive(Deserialize)]
pub struct LookUpUserByGithubLoginBody {
    github_login: String,
}

#[derive(Serialize)]
pub struct UserResponse {
    user: Option<LocalUser>,
}

#[derive(Deserialize)]
pub struct FuzzySearchUsersBody {
    query: String,
    limit: u32,
}

#[derive(Deserialize)]
pub struct FuzzySearchChannelMembersBody {
    channel_id: i32,
    query: String,
    limit: u32,
}

#[derive(Serialize)]
pub struct ChannelMembersResponse {
    channel_members: Vec<ChannelMember>,
    users: Vec<LocalUser>,
}

#[derive(Serialize)]
pub struct ChannelMember {
    legacy_user_id: i32,
    kind: &'static str,
    role: String,
}

#[route(GET "/healthz")]
async fn health() -> Result<StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}

#[route(GET "/models")]
async fn models() -> Result<Json<Value>> {
    Ok(Json(json!({
        "models": [],
        "default_model": null,
        "default_fast_model": null,
        "recommended_models": []
    })))
}

#[route(GET "/client/users/me")]
async fn authenticated_user(cx: &Cx) -> Result<Json<Value>> {
    let state = app_context::<AppState>(cx);
    let credentials = parse_authorization_header(headers(cx))?;
    let user = store::authenticated_user(
        &state.database,
        credentials.user_id,
        &credentials.access_token,
    )
    .await
    .map_err(|error| internal("failed to load the authenticated user", error))?
    .ok_or_else(unauthorized)?;

    Ok(Json(authenticated_user_response(&user)))
}

#[route(GET "/client/users/connect")]
async fn users_connect(cx: &Cx, websocket: WebSocketUpgrade) -> Result<Response> {
    validate_cached_credentials(cx).await?;
    websocket.on_upgrade(keep_cloud_connection_alive)
}

async fn keep_cloud_connection_alive(mut socket: WebSocket) {
    while let Some(message) = socket.recv().await {
        match message {
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

#[route([PATCH, POST] "/client/system_settings")]
async fn update_system_settings(cx: &Cx, Json(body): Json<Value>) -> Result<Json<Value>> {
    validate_cached_credentials(cx).await?;
    Ok(Json(json!({
        "selected_organization_id": body
            .get("selected_organization_id")
            .cloned()
            .unwrap_or(Value::Null)
    })))
}

#[route(POST "/client/llm_tokens")]
async fn llm_tokens(cx: &Cx) -> Result<Json<Value>> {
    validate_cached_credentials(cx).await?;
    Ok(Json(json!({"token": "local-ai-disabled"})))
}

#[route(POST "/client/feedback/{*path}")]
async fn ignore_feedback(cx: &Cx) -> Result<StatusCode> {
    validate_cached_credentials(cx).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[route(POST "/internal/users/look_up_by_legacy_id")]
async fn look_up_users_by_legacy_id(
    cx: &Cx,
    Json(body): Json<LookUpUsersByLegacyIdBody>,
) -> Result<Json<UsersResponse>> {
    let state = app_context::<AppState>(cx);
    validate_internal_api_key(state, headers(cx))?;
    let users = store::users_by_ids(&state.database, &body.legacy_user_ids)
        .await
        .map_err(|error| internal("failed to look up users", error))?;
    Ok(Json(UsersResponse { users }))
}

#[route(POST "/internal/users/look_up_by_github_login")]
async fn look_up_user_by_github_login(
    cx: &Cx,
    Json(body): Json<LookUpUserByGithubLoginBody>,
) -> Result<Json<UserResponse>> {
    let state = app_context::<AppState>(cx);
    validate_internal_api_key(state, headers(cx))?;
    let user = store::user_by_github_login(&state.database, &body.github_login)
        .await
        .map_err(|error| internal("failed to look up a user", error))?;
    Ok(Json(UserResponse { user }))
}

#[route(POST "/internal/users/fuzzy_search")]
async fn fuzzy_search_users(
    cx: &Cx,
    Json(body): Json<FuzzySearchUsersBody>,
) -> Result<Json<UsersResponse>> {
    let state = app_context::<AppState>(cx);
    validate_internal_api_key(state, headers(cx))?;
    let users = store::fuzzy_search_users(&state.database, &body.query, search_limit(body.limit))
        .await
        .map_err(|error| internal("failed to search users", error))?;
    Ok(Json(UsersResponse { users }))
}

#[route(POST "/internal/channel_members/fuzzy_search")]
async fn fuzzy_search_channel_members(
    cx: &Cx,
    Json(body): Json<FuzzySearchChannelMembersBody>,
) -> Result<Json<ChannelMembersResponse>> {
    let state = app_context::<AppState>(cx);
    validate_internal_api_key(state, headers(cx))?;
    let rows = store::fuzzy_search_channel_members(
        &state.database,
        body.channel_id,
        &body.query,
        search_limit(body.limit),
    )
    .await
    .map_err(|error| internal("failed to search channel members", error))?;

    let mut channel_members = Vec::with_capacity(rows.len());
    let mut users = Vec::with_capacity(rows.len());
    for row in rows {
        channel_members.push(ChannelMember {
            legacy_user_id: row.user_id,
            kind: if row.accepted { "member" } else { "invitee" },
            role: row.role.clone(),
        });
        users.push(LocalUser {
            user_id: row.user_id,
            stable_id: row.stable_id,
            username: row.username,
            github_login: row.github_login,
            avatar_url: row.avatar_url,
            name: row.name,
            admin: row.admin,
            connected_once: row.connected_once,
        });
    }

    Ok(Json(ChannelMembersResponse {
        channel_members,
        users,
    }))
}

pub fn parse_authorization_header(headers: &HeaderMap) -> Result<Credentials> {
    let value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(unauthorized)?
        .to_str()
        .map_err(|_| unauthorized())?;
    let mut parts = value.split_whitespace();
    let user_id = parts
        .next()
        .ok_or_else(unauthorized)?
        .parse::<i32>()
        .map_err(|_| unauthorized())?;
    if user_id <= 0 {
        return Err(unauthorized().into());
    }
    let access_token = parts
        .next()
        .filter(|token| !token.is_empty())
        .ok_or_else(unauthorized)?;
    if parts.next().is_some() {
        return Err(unauthorized().into());
    }

    Ok(Credentials {
        user_id,
        access_token: access_token.to_owned(),
    })
}

async fn validate_cached_credentials(cx: &Cx) -> Result<()> {
    let state = app_context::<AppState>(cx);
    let credentials = parse_authorization_header(headers(cx))?;
    let valid = store::cached_token_is_valid(
        &state.database,
        credentials.user_id,
        &credentials.access_token,
    )
    .await
    .map_err(|error| internal("failed to validate cached credentials", error))?;
    if !valid {
        return Err(unauthorized().into());
    }
    Ok(())
}

fn validate_internal_api_key(state: &AppState, headers: &HeaderMap) -> Result<()> {
    let expected = format!("Bearer {}", state.config.internal_api_key);
    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected);
    if authorized {
        Ok(())
    } else {
        Err(unauthorized().into())
    }
}

fn authenticated_user_response(user: &store::AuthenticatedLocalUser) -> Value {
    json!({
        "user": {
            "id_v2": user.stable_id,
            "legacy_user_id": user.user_id,
            "metrics_id": user.metrics_id.to_string(),
            "username": user.username,
            "avatar_url": user.avatar_url,
            "github_login": user.github_login,
            "name": user.name,
            "is_staff": user.admin,
            "accepted_tos_at": null,
            "has_connected_to_collab_once": user.connected_once
        },
        "feature_flags": [],
        "organizations": [{"id": "local", "name": "Local", "is_personal": true}],
        "default_organization_id": "local",
        "plans_by_organization": {"local": "zed_free"},
        "configuration_by_organization": {
            "local": {
                "is_zed_model_provider_enabled": false,
                "is_agent_thread_feedback_enabled": false,
                "is_collaboration_enabled": true,
                "edit_prediction": {
                    "is_enabled": false,
                    "is_feedback_enabled": false
                }
            }
        },
        "plan": {
            "plan_v3": "zed_free",
            "subscription_period": null,
            "usage": {
                "edit_predictions": {"used": 0, "limit": "unlimited"}
            },
            "trial_started_at": null,
            "is_account_too_young": false,
            "has_overdue_invoices": false
        }
    })
}

fn search_limit(requested: u32) -> i64 {
    i64::from(requested.min(100))
}

fn internal(context: &'static str, error: impl Into<anyhow::Error>) -> topcoat::Error {
    let error = error.into().context(context);
    tracing::error!(%error, "request failed");
    internal_server_error(error).into()
}

#[cfg(test)]
mod tests {
    use topcoat::router::HeaderValue;

    use super::*;

    #[test]
    fn parses_zed_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("42 access-token"),
        );
        let credentials = parse_authorization_header(&headers).expect("header should parse");
        assert_eq!(credentials.user_id, 42);
        assert_eq!(credentials.access_token, "access-token");
    }

    #[test]
    fn rejects_extra_authorization_fields() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("42 access-token extra"),
        );
        assert!(parse_authorization_header(&headers).is_err());
    }
}

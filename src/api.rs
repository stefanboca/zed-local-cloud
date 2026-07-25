use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Query, State, WebSocketUpgrade, ws},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::{
    app::{AppError, AppState},
    config::url_with_path,
    store::{self, AuthenticatedProfile, LocalUser},
};

#[derive(Debug)]
pub struct Credentials {
    pub user_id: i32,
    pub access_token: String,
}

#[derive(Deserialize)]
pub struct NativeAppSignInQuery {
    #[serde(flatten)]
    parameters: BTreeMap<String, String>,
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

pub async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}

pub async fn native_app_sign_in(
    State(state): State<AppState>,
    Query(query): Query<NativeAppSignInQuery>,
) -> Result<Redirect, AppError> {
    let query = serde_urlencoded::to_string(query.parameters)
        .map_err(|error| AppError::internal("failed to encode sign-in query", error))?;
    let url = url_with_path(
        &state.config.upstream_web_url,
        "/native_app_signin",
        Some(&query),
    );
    Ok(Redirect::temporary(url.as_str()))
}

pub async fn authenticated_user(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let credentials = parse_authorization_header(&headers)?;
    let authorization = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| AppError::unauthorized("missing Authorization header"))?
        .clone();
    let url = url_with_path(&state.config.upstream_cloud_url, "/client/users/me", None);

    let mut request = state
        .http_client
        .get(url)
        .header(header::AUTHORIZATION, authorization);
    if let Some(system_id) = headers.get("x-zed-system-id") {
        request = request.header("x-zed-system-id", system_id);
    }
    if let Some(user_agent) = headers.get(header::USER_AGENT) {
        request = request.header(header::USER_AGENT, user_agent);
    }

    let upstream = request
        .send()
        .await
        .map_err(|error| AppError::bad_gateway("upstream auth request failed", error))?;
    let status = upstream.status();
    let content_type = upstream.headers().get(header::CONTENT_TYPE).cloned();
    let body = upstream
        .bytes()
        .await
        .map_err(|error| AppError::bad_gateway("failed to read upstream auth response", error))?;

    if !status.is_success() {
        let mut response = Response::builder().status(status);
        if let Some(content_type) = content_type {
            response = response.header(header::CONTENT_TYPE, content_type);
        }
        return response
            .body(body.into())
            .map_err(|error| AppError::internal("failed to build upstream response", error));
    }

    let mut response: Value = serde_json::from_slice(&body)
        .map_err(|error| AppError::bad_gateway("upstream returned invalid user JSON", error))?;
    let profile = profile_from_response(&response)?;
    if profile.user_id != credentials.user_id {
        return Err(AppError::unauthorized(
            "upstream user did not match the Authorization header",
        ));
    }

    let admin = state.config.is_admin(&profile.github_login);
    let connected_once =
        store::sync_authenticated_user(&state.database, &profile, &credentials.access_token, admin)
            .await
            .map_err(|error| AppError::internal("failed to synchronize local user", error))?;
    make_local_collaboration_response(&mut response, connected_once, admin)?;

    Ok(Json(response).into_response())
}

pub async fn users_connect(
    State(state): State<AppState>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Result<Response, AppError> {
    validate_cached_credentials(&state, &headers).await?;
    Ok(websocket.on_upgrade(keep_cloud_connection_alive))
}

async fn keep_cloud_connection_alive(mut socket: ws::WebSocket) {
    while let Some(message) = socket.recv().await {
        match message {
            Ok(ws::Message::Ping(payload)) => {
                if socket.send(ws::Message::Pong(payload)).await.is_err() {
                    break;
                }
            }
            Ok(ws::Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

pub async fn update_system_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    validate_cached_credentials(&state, &headers).await?;
    Ok(Json(json!({
        "selected_organization_id": body
            .get("selected_organization_id")
            .cloned()
            .unwrap_or(Value::Null)
    })))
}

pub async fn llm_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    validate_cached_credentials(&state, &headers).await?;
    Ok(Json(json!({"token": "local-ai-disabled"})))
}

pub async fn ignore_feedback(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    validate_cached_credentials(&state, &headers).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn look_up_users_by_legacy_id(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LookUpUsersByLegacyIdBody>,
) -> Result<Json<UsersResponse>, AppError> {
    validate_internal_api_key(&state, &headers)?;
    let users = store::users_by_ids(&state.database, &body.legacy_user_ids)
        .await
        .map_err(|error| AppError::internal("failed to look up users", error))?;
    Ok(Json(UsersResponse { users }))
}

pub async fn look_up_user_by_github_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LookUpUserByGithubLoginBody>,
) -> Result<Json<UserResponse>, AppError> {
    validate_internal_api_key(&state, &headers)?;
    let user = store::user_by_github_login(&state.database, &body.github_login)
        .await
        .map_err(|error| AppError::internal("failed to look up user", error))?;
    Ok(Json(UserResponse { user }))
}

pub async fn fuzzy_search_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<FuzzySearchUsersBody>,
) -> Result<Json<UsersResponse>, AppError> {
    validate_internal_api_key(&state, &headers)?;
    let users = store::fuzzy_search_users(&state.database, &body.query, search_limit(body.limit))
        .await
        .map_err(|error| AppError::internal("failed to search users", error))?;
    Ok(Json(UsersResponse { users }))
}

pub async fn fuzzy_search_channel_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<FuzzySearchChannelMembersBody>,
) -> Result<Json<ChannelMembersResponse>, AppError> {
    validate_internal_api_key(&state, &headers)?;
    let rows = store::fuzzy_search_channel_members(
        &state.database,
        body.channel_id,
        &body.query,
        search_limit(body.limit),
    )
    .await
    .map_err(|error| AppError::internal("failed to search channel members", error))?;

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

pub fn parse_authorization_header(headers: &HeaderMap) -> Result<Credentials, AppError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| AppError::unauthorized("missing Authorization header"))?
        .to_str()
        .map_err(|_| AppError::unauthorized("invalid Authorization header"))?;
    let mut parts = value.split_whitespace();
    let user_id = parts
        .next()
        .ok_or_else(|| AppError::unauthorized("invalid Authorization header"))?
        .parse::<i32>()
        .map_err(|_| AppError::unauthorized("invalid user id in Authorization header"))?;
    if user_id <= 0 {
        return Err(AppError::unauthorized(
            "invalid user id in Authorization header",
        ));
    }
    let access_token = parts
        .next()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| AppError::unauthorized("missing access token"))?;
    if parts.next().is_some() {
        return Err(AppError::unauthorized("invalid Authorization header"));
    }

    Ok(Credentials {
        user_id,
        access_token: access_token.to_owned(),
    })
}

async fn validate_cached_credentials(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), AppError> {
    let credentials = parse_authorization_header(headers)?;
    let valid = store::cached_token_is_valid(
        &state.database,
        credentials.user_id,
        &credentials.access_token,
    )
    .await
    .map_err(|error| AppError::internal("failed to validate cached credentials", error))?;
    if !valid {
        return Err(AppError::unauthorized(
            "credentials have not been validated",
        ));
    }
    Ok(())
}

fn validate_internal_api_key(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let expected = format!("Bearer {}", state.config.internal_api_key);
    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected);
    if authorized {
        Ok(())
    } else {
        Err(AppError::unauthorized("invalid internal API key"))
    }
}

fn profile_from_response(response: &Value) -> Result<AuthenticatedProfile, AppError> {
    let user = response
        .get("user")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::bad_gateway_message("upstream response has no user object"))?;

    let user_id = required_i32(user, "id")?;
    let metrics_id = required_string(user, "metrics_id")?
        .parse::<Uuid>()
        .map_err(|error| AppError::bad_gateway("upstream metrics_id is not a UUID", error))?;

    Ok(AuthenticatedProfile {
        user_id,
        github_login: required_string(user, "github_login")?.to_owned(),
        avatar_url: required_string(user, "avatar_url")?.to_owned(),
        name: optional_string(user, "name")?,
        email_address: optional_string(user, "email_address")?,
        metrics_id,
        upstream_is_staff: required_bool(user, "is_staff")?,
        upstream_connected_once: required_bool(user, "has_connected_to_collab_once")?,
    })
}

fn make_local_collaboration_response(
    response: &mut Value,
    connected_once: bool,
    admin: bool,
) -> Result<(), AppError> {
    let object = response
        .as_object_mut()
        .ok_or_else(|| AppError::bad_gateway_message("upstream response is not an object"))?;
    let user = object
        .get_mut("user")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::bad_gateway_message("upstream response has no user object"))?;
    user.insert(
        "has_connected_to_collab_once".to_owned(),
        Value::Bool(connected_once),
    );
    user.insert("is_staff".to_owned(), Value::Bool(admin));

    object.insert(
        "organizations".to_owned(),
        json!([{"id": "local", "name": "Local", "is_personal": true}]),
    );
    object.insert(
        "default_organization_id".to_owned(),
        Value::String("local".to_owned()),
    );
    object.insert(
        "plans_by_organization".to_owned(),
        json!({"local": "zed_free"}),
    );
    object.insert(
        "configuration_by_organization".to_owned(),
        json!({
            "local": {
                "is_zed_model_provider_enabled": false,
                "is_agent_thread_feedback_enabled": false,
                "is_collaboration_enabled": true,
                "edit_prediction": {
                    "is_enabled": false,
                    "is_feedback_enabled": false
                }
            }
        }),
    );
    Ok(())
}

fn required_string<'a>(object: &'a Map<String, Value>, name: &str) -> Result<&'a str, AppError> {
    object
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_gateway_message(format!("upstream user has invalid {name}")))
}

fn optional_string(object: &Map<String, Value>, name: &str) -> Result<Option<String>, AppError> {
    match object.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(AppError::bad_gateway_message(format!(
            "upstream user has invalid {name}"
        ))),
    }
}

fn required_i32(object: &Map<String, Value>, name: &str) -> Result<i32, AppError> {
    object
        .get(name)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| AppError::bad_gateway_message(format!("upstream user has invalid {name}")))
}

fn required_bool(object: &Map<String, Value>, name: &str) -> Result<bool, AppError> {
    object
        .get(name)
        .and_then(Value::as_bool)
        .ok_or_else(|| AppError::bad_gateway_message(format!("upstream user has invalid {name}")))
}

fn search_limit(requested: u32) -> i64 {
    i64::from(requested.min(100))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::Arc};

    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{HeaderValue, Request},
        routing::get,
    };
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt as _;
    use url::Url;

    use super::*;
    use crate::{
        app::{App, AppState},
        config::Config,
    };

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

    #[test]
    fn enables_only_local_collaboration() {
        let mut response = json!({
            "user": {"has_connected_to_collab_once": false},
            "organizations": []
        });
        make_local_collaboration_response(&mut response, true, false)
            .expect("response should be transformable");

        assert_eq!(
            response["configuration_by_organization"]["local"]["is_collaboration_enabled"],
            true
        );
        assert_eq!(
            response["user"]["has_connected_to_collab_once"],
            Value::Bool(true)
        );
        assert_eq!(response["user"]["is_staff"], Value::Bool(false));
    }

    #[test]
    fn validates_the_upstream_profile_shape() {
        let response = json!({
            "user": {
                "id": 42,
                "metrics_id": "936da01f-9abd-4d9d-80c7-02af85c822a8",
                "avatar_url": "https://example.com/avatar.png",
                "github_login": "local-user",
                "name": "Local User",
                "is_staff": false,
                "has_connected_to_collab_once": true
            }
        });
        let profile = profile_from_response(&response).expect("valid upstream profile");

        assert_eq!(profile.user_id, 42);
        assert_eq!(profile.github_login, "local-user");
        assert_eq!(profile.name.as_deref(), Some("Local User"));
        assert!(profile.upstream_connected_once);
    }

    #[test]
    fn rejects_an_invalid_upstream_metrics_id() {
        let response = json!({
            "user": {
                "id": 42,
                "metrics_id": "not-a-uuid",
                "avatar_url": "",
                "github_login": "local-user",
                "name": null,
                "is_staff": false,
                "has_connected_to_collab_once": false
            }
        });

        assert!(profile_from_response(&response).is_err());
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing to a disposable collab-schema database"]
    async fn validates_upstream_credentials_and_synchronizes_the_local_user() {
        let database_url =
            std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL should be set");
        let database = PgPoolOptions::new()
            .connect(&database_url)
            .await
            .expect("test database should be reachable");
        sqlx::migrate!()
            .run(&database)
            .await
            .expect("facade migrations should run");
        sqlx::query("DELETE FROM users WHERE id = 42")
            .execute(&database)
            .await
            .expect("test user cleanup should succeed");

        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fake upstream listener should bind");
        let upstream_address = upstream_listener
            .local_addr()
            .expect("fake upstream listener should have an address");
        let upstream = Router::new().route(
            "/client/users/me",
            get(|headers: HeaderMap| async move {
                if headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    != Some("42 access-token")
                {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                Json(json!({
                    "user": {
                        "id": 42,
                        "metrics_id": "936da01f-9abd-4d9d-80c7-02af85c822a8",
                        "avatar_url": "https://example.com/avatar.png",
                        "github_login": "local-user",
                        "name": "Local User",
                        "is_staff": false,
                        "accepted_tos_at": null,
                        "has_connected_to_collab_once": false
                    },
                    "feature_flags": [],
                    "organizations": [],
                    "default_organization_id": null,
                    "plans_by_organization": {},
                    "configuration_by_organization": {},
                    "plan": {
                        "plan_v3": "zed_free",
                        "subscription_period": null,
                        "usage": {
                            "edit_predictions": {
                                "used": 0,
                                "limit": "unlimited"
                            }
                        },
                        "trial_started_at": null,
                        "is_account_too_young": false,
                        "has_overdue_invoices": false
                    }
                }))
                .into_response()
            }),
        );
        let upstream_task = tokio::spawn(async move {
            axum::serve(upstream_listener, upstream)
                .await
                .expect("fake upstream should run");
        });

        let mut config = Config::test();
        config.upstream_cloud_url =
            Url::parse(&format!("http://{upstream_address}")).expect("valid upstream URL");
        config.admin_github_logins = HashSet::from(["local-user".to_owned()]);
        let state = AppState {
            config: Arc::new(config),
            database: database.clone(),
            http_client: reqwest::Client::new(),
        };
        let response = App::router(state)
            .oneshot(
                Request::builder()
                    .uri("/client/users/me")
                    .header(header::AUTHORIZATION, "42 access-token")
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("facade should respond");
        assert_eq!(response.status(), StatusCode::OK);
        let response: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), 1024 * 1024)
                .await
                .expect("response body should be readable"),
        )
        .expect("response should contain JSON");
        assert_eq!(response["user"]["is_staff"], true);
        assert_eq!(
            response["configuration_by_organization"]["local"]["is_collaboration_enabled"],
            true
        );

        let user = store::user_by_github_login(&database, "LOCAL-USER")
            .await
            .expect("user lookup should succeed")
            .expect("synchronized user should exist");
        assert_eq!(user.user_id, 42);
        assert!(user.admin);
        assert!(
            store::cached_token_is_valid(&database, 42, "access-token")
                .await
                .expect("token lookup should succeed")
        );
        assert!(
            !store::cached_token_is_valid(&database, 42, "wrong-token")
                .await
                .expect("token lookup should succeed")
        );

        sqlx::query("DELETE FROM users WHERE id = 42")
            .execute(&database)
            .await
            .expect("test user cleanup should succeed");
        upstream_task.abort();
    }
}

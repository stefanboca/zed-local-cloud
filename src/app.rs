use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use axum::{
    Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{any, get, patch, post},
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower_http::trace::TraceLayer;

use crate::{api, config::Config, proxy};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub database: PgPool,
    pub http_client: reqwest::Client,
}

pub struct App {
    state: AppState,
}

impl App {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let database = PgPoolOptions::new()
            .max_connections(10)
            .connect(&config.database_url)
            .await
            .context("failed to connect to Postgres")?;
        sqlx::migrate!()
            .run(&database)
            .await
            .context("failed to run database migrations")?;

        let http_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("failed to construct HTTP client")?;

        Ok(Self {
            state: AppState {
                config: Arc::new(config),
                database,
                http_client,
            },
        })
    }

    pub fn router(state: AppState) -> Router {
        Router::new()
            .route("/healthz", get(api::health))
            .route("/native_app_signin", get(api::native_app_sign_in))
            .route(
                "/native_app_signin_succeeded",
                get(api::native_app_sign_in_succeeded),
            )
            .route("/rpc", get(api::rpc_redirect))
            .route("/actual_rpc", get(proxy::actual_rpc))
            .route("/client/users/me", get(api::authenticated_user))
            .route("/client/users/connect", get(api::users_connect))
            .route(
                "/client/system_settings",
                patch(api::update_system_settings).post(api::update_system_settings),
            )
            .route("/client/llm_tokens", post(api::llm_tokens))
            .route("/client/feedback/{*path}", post(api::ignore_feedback))
            .route(
                "/internal/users/look_up_by_legacy_id",
                post(api::look_up_users_by_legacy_id),
            )
            .route(
                "/internal/users/look_up_by_github_login",
                post(api::look_up_user_by_github_login),
            )
            .route(
                "/internal/users/fuzzy_search",
                post(api::fuzzy_search_users),
            )
            .route(
                "/internal/channel_members/fuzzy_search_by_github_login",
                post(api::fuzzy_search_channel_members),
            )
            .route("/extensions", any(proxy::extensions_proxy))
            .route("/extensions/{*path}", any(proxy::extensions_proxy))
            .fallback(proxy::fallback_proxy)
            .layer(TraceLayer::new_for_http())
            .with_state(state)
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        let bind_address = self.state.config.bind_address;
        let listener = tokio::net::TcpListener::bind(bind_address)
            .await
            .with_context(|| format!("failed to bind {bind_address}"))?;
        tracing::info!(
            %bind_address,
            public_base_url = %self.state.config.public_base_url,
            local_collab_url = %self.state.config.local_collab_url,
            "Zed auth facade listening"
        );
        axum::serve(listener, Self::router(self.state))
            .with_graceful_shutdown(shutdown_signal())
            .await
            .context("HTTP server failed")
    }
}

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    public_message: String,
    source: Option<anyhow::Error>,
}

impl AppError {
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            public_message: message.into(),
            source: None,
        }
    }

    pub fn bad_gateway(message: impl Into<String>, source: impl Into<anyhow::Error>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            public_message: message.into(),
            source: Some(source.into()),
        }
    }

    pub fn bad_gateway_message(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            public_message: message.into(),
            source: None,
        }
    }

    pub fn internal(message: impl Into<String>, source: impl Into<anyhow::Error>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            public_message: message.into(),
            source: Some(source.into()),
        }
    }

    pub fn internal_message(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            public_message: message.into(),
            source: None,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Some(source) = &self.source {
            tracing::error!(
                status = self.status.as_u16(),
                message = %self.public_message,
                error = %source,
                "request failed"
            );
        } else {
            tracing::warn!(
                status = self.status.as_u16(),
                message = %self.public_message,
                "request rejected"
            );
        }
        (self.status, self.public_message).into_response()
    }
}

async fn shutdown_signal() {
    let control_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to listen for Ctrl-C");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "failed to listen for SIGTERM"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = control_c => {}
        () = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt as _;

    use super::*;

    #[tokio::test]
    async fn rpc_route_redirects_to_the_facade_proxy() {
        let config = Config::test();
        let database = PgPoolOptions::new()
            .connect_lazy(&config.database_url)
            .expect("valid test database URL");
        let state = AppState {
            config: Arc::new(config),
            database,
            http_client: reqwest::Client::new(),
        };

        let response = App::router(state)
            .oneshot(
                Request::builder()
                    .uri("/rpc")
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers().get(header::LOCATION),
            Some(
                &"http://localhost:8787/actual_rpc"
                    .parse()
                    .expect("valid header")
            )
        );
    }
}

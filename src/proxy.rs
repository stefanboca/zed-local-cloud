use axum::{
    body::Body,
    extract::{Request, State, WebSocketUpgrade, ws},
    http::{HeaderMap, HeaderName, header},
    response::Response,
};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Message as UpstreamMessage,
        client::IntoClientRequest as _,
        protocol::{CloseFrame as UpstreamCloseFrame, frame::coding::CloseCode},
    },
};
use url::Url;

use crate::{
    app::{AppError, AppState},
    config::url_with_path,
};

pub async fn actual_rpc(
    State(state): State<AppState>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Result<Response, AppError> {
    let mut upstream_url = url_with_path(&state.config.local_collab_url, "/rpc", None);
    upstream_url
        .set_scheme(match upstream_url.scheme() {
            "http" => "ws",
            "https" => "wss",
            scheme => {
                return Err(AppError::internal_message(format!(
                    "unsupported collab URL scheme {scheme}"
                )));
            }
        })
        .map_err(|_| AppError::internal_message("failed to construct collab WebSocket URL"))?;

    let mut request = upstream_url
        .as_str()
        .into_client_request()
        .map_err(|error| AppError::internal("failed to build collab WebSocket request", error))?;
    copy_request_headers(&headers, request.headers_mut());
    let (upstream, _) = connect_async(request)
        .await
        .map_err(|error| AppError::bad_gateway("failed to connect to local collab", error))?;

    Ok(websocket.on_upgrade(move |downstream| bridge_websockets(downstream, upstream)))
}

pub async fn extensions_proxy(
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, AppError> {
    proxy_http(&state, request, state.config.upstream_api_url.clone()).await
}

pub async fn fallback_proxy(
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, AppError> {
    let base_url = if request.uri().path().starts_with("/client/") {
        state.config.upstream_cloud_url.clone()
    } else {
        state.config.upstream_web_url.clone()
    };
    proxy_http(&state, request, base_url).await
}

async fn proxy_http(
    state: &AppState,
    request: Request,
    base_url: Url,
) -> Result<Response, AppError> {
    let (parts, body) = request.into_parts();
    let target = url_with_path(&base_url, parts.uri.path(), parts.uri.query());
    let mut upstream = state
        .http_client
        .request(parts.method, target)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()));
    for (name, value) in &parts.headers {
        if !is_hop_by_hop(name) && name != header::HOST {
            upstream = upstream.header(name, value);
        }
    }

    let response = upstream
        .send()
        .await
        .map_err(|error| AppError::bad_gateway("upstream proxy request failed", error))?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = Body::from_stream(response.bytes_stream());
    let mut downstream = Response::builder()
        .status(status)
        .body(body)
        .map_err(|error| {
            AppError::internal("failed to construct upstream proxy response", error)
        })?;
    for (name, value) in &headers {
        if !is_hop_by_hop(name) {
            downstream.headers_mut().append(name, value.clone());
        }
    }
    Ok(downstream)
}

fn copy_request_headers(source: &HeaderMap, destination: &mut HeaderMap) {
    for (name, value) in source {
        if !is_websocket_handshake_header(name) && name != header::HOST {
            destination.append(name, value.clone());
        }
    }
}

fn is_hop_by_hop(name: &HeaderName) -> bool {
    name == header::CONNECTION
        || name.as_str() == "keep-alive"
        || name == header::PROXY_AUTHENTICATE
        || name == header::PROXY_AUTHORIZATION
        || name == header::TE
        || name == header::TRAILER
        || name == header::TRANSFER_ENCODING
        || name == header::UPGRADE
}

fn is_websocket_handshake_header(name: &HeaderName) -> bool {
    is_hop_by_hop(name)
        || name == header::SEC_WEBSOCKET_ACCEPT
        || name == header::SEC_WEBSOCKET_KEY
        || name == header::SEC_WEBSOCKET_VERSION
        || name.as_str() == "sec-websocket-extensions"
}

async fn bridge_websockets(
    downstream: ws::WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    let (mut downstream_sender, mut downstream_receiver) = downstream.split();
    let (mut upstream_sender, mut upstream_receiver) = upstream.split();

    let downstream_to_upstream = async {
        while let Some(message) = downstream_receiver.next().await {
            let message = message.map_err(anyhow::Error::from)?;
            upstream_sender
                .send(to_upstream_message(message))
                .await
                .map_err(anyhow::Error::from)?;
        }
        anyhow::Ok(())
    };

    let upstream_to_downstream = async {
        while let Some(message) = upstream_receiver.next().await {
            let message = message.map_err(anyhow::Error::from)?;
            if let Some(message) = to_downstream_message(message) {
                downstream_sender
                    .send(message)
                    .await
                    .map_err(anyhow::Error::from)?;
            }
        }
        anyhow::Ok(())
    };

    let result = tokio::select! {
        result = downstream_to_upstream => result,
        result = upstream_to_downstream => result,
    };
    if let Err(error) = result {
        tracing::debug!(%error, "WebSocket proxy closed with an error");
    }
}

fn to_upstream_message(message: ws::Message) -> UpstreamMessage {
    match message {
        ws::Message::Text(text) => UpstreamMessage::Text(text.as_str().into()),
        ws::Message::Binary(data) => UpstreamMessage::Binary(data),
        ws::Message::Ping(data) => UpstreamMessage::Ping(data),
        ws::Message::Pong(data) => UpstreamMessage::Pong(data),
        ws::Message::Close(frame) => {
            UpstreamMessage::Close(frame.map(|frame| UpstreamCloseFrame {
                code: CloseCode::from(frame.code),
                reason: frame.reason.as_str().into(),
            }))
        }
    }
}

fn to_downstream_message(message: UpstreamMessage) -> Option<ws::Message> {
    match message {
        UpstreamMessage::Text(text) => Some(ws::Message::Text(text.as_str().into())),
        UpstreamMessage::Binary(data) => Some(ws::Message::Binary(data)),
        UpstreamMessage::Ping(data) => Some(ws::Message::Ping(data)),
        UpstreamMessage::Pong(data) => Some(ws::Message::Pong(data)),
        UpstreamMessage::Close(frame) => {
            Some(ws::Message::Close(frame.map(|frame| ws::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.as_str().into(),
            })))
        }
        UpstreamMessage::Frame(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use axum::http::HeaderValue;
    use sqlx::postgres::PgPoolOptions;
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::{
            Message,
            handshake::server::{Request, Response},
        },
    };

    use super::*;
    use crate::{
        app::{App, AppState},
        config::Config,
    };

    #[tokio::test]
    #[expect(
        clippy::result_large_err,
        reason = "the tungstenite handshake callback requires this response type"
    )]
    async fn websocket_proxy_forwards_headers_and_messages() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("upstream listener should bind");
        let upstream_address = upstream_listener
            .local_addr()
            .expect("upstream listener should have an address");
        let upstream_task = tokio::spawn(async move {
            let (stream, _) = upstream_listener
                .accept()
                .await
                .expect("upstream should accept a connection");
            let mut authorization = None;
            let mut websocket =
                accept_hdr_async(stream, |request: &Request, response: Response| {
                    authorization = request.headers().get(header::AUTHORIZATION).cloned();
                    Ok(response)
                })
                .await
                .expect("upstream WebSocket handshake should succeed");
            let message = websocket
                .next()
                .await
                .expect("upstream should receive a message")
                .expect("upstream message should be valid");
            websocket
                .send(message)
                .await
                .expect("upstream should echo the message");
            authorization
        });

        let mut config = Config::test();
        config.local_collab_url =
            Url::parse(&format!("http://{upstream_address}")).expect("valid upstream URL");
        let database = PgPoolOptions::new()
            .connect_lazy(&config.database_url)
            .expect("valid test database URL");
        let state = AppState {
            config: Arc::new(config),
            database,
            http_client: reqwest::Client::new(),
        };
        let facade_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("facade listener should bind");
        let facade_address = facade_listener
            .local_addr()
            .expect("facade listener should have an address");
        let facade_task = tokio::spawn(async move {
            axum::serve(facade_listener, App::router(state))
                .await
                .expect("facade server should run");
        });

        let mut request = format!("ws://{facade_address}/actual_rpc")
            .into_client_request()
            .expect("valid facade WebSocket request");
        request.headers_mut().insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("42 access-token"),
        );
        let (mut websocket, _) = connect_async(request)
            .await
            .expect("facade WebSocket handshake should succeed");
        websocket
            .send(Message::Binary(vec![1, 2, 3].into()))
            .await
            .expect("client should send a message");
        let echoed = tokio::time::timeout(Duration::from_secs(2), websocket.next())
            .await
            .expect("proxy should respond before timeout")
            .expect("proxy should return a message")
            .expect("proxied message should be valid");

        assert_eq!(echoed, Message::Binary(vec![1, 2, 3].into()));
        let authorization = upstream_task
            .await
            .expect("upstream task should complete")
            .expect("Authorization should be forwarded");
        assert_eq!(authorization, "42 access-token");
        facade_task.abort();
    }
}

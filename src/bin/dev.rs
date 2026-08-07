use anyhow::Context as _;
use topcoat::{
    Result,
    router::{
        Router,
        error::{SeeOther, see_other},
        route,
    },
    view::view,
};
use zed_local_cloud::ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    topcoat::start(router())
        .await
        .context("UI development server failed")
}

fn router() -> Router {
    Router::builder()
        .route(preview_index)
        .route(preview_sign_in)
        .route(preview_start_oidc)
        .route(preview_sign_in_succeeded)
        .build()
}

#[route(GET "/")]
async fn preview_index() -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Zed local cloud UI previews"</title>
                topcoat::dev::script()
            </head>
            <body>
                <main>
                    <h1>"Zed local cloud UI previews"</h1>
                    <ul>
                        <li><a href="/native_app_signin">"Sign in"</a></li>
                        <li>
                            <a href="/native_app_signin_succeeded">
                                "Sign-in complete"
                            </a>
                        </li>
                    </ul>
                </main>
            </body>
        </html>
    }
}

#[route(GET "/native_app_signin")]
async fn preview_sign_in(cx: &topcoat::context::Cx) -> Result {
    ui::sign_in(cx, true).await
}

#[route(POST "/auth/oidc/start")]
async fn preview_start_oidc() -> Result<SeeOther> {
    Ok(see_other("/native_app_signin_succeeded"))
}

#[route(GET "/native_app_signin_succeeded")]
async fn preview_sign_in_succeeded(cx: &topcoat::context::Cx) -> Result {
    ui::sign_in_succeeded(cx, true).await
}

#[cfg(test)]
mod tests {
    use topcoat::router::{Body, Method, Request, StatusCode, header, to_bytes};

    use super::*;

    async fn send(method: Method, path: &str) -> topcoat::router::Response {
        router()
            .handle(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .expect("valid preview request"),
            )
            .await
    }

    #[tokio::test]
    async fn serves_the_preview_pages_without_application_state() {
        let response = send(Method::GET, "/").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read preview response");
        assert!(String::from_utf8_lossy(&body).contains("do not use OIDC or PostgreSQL"));

        let response = send(Method::GET, "/native_app_signin").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read sign-in response");
        assert!(String::from_utf8_lossy(&body).contains("Continue with OpenID Connect"));

        let response = send(Method::GET, "/native_app_signin_succeeded").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read success response");
        assert!(String::from_utf8_lossy(&body).contains("Sign-in complete"));
    }

    #[tokio::test]
    async fn mocks_the_sign_in_transition_and_excludes_application_routes() {
        let response = send(Method::POST, "/auth/oidc/start").await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers().get(header::LOCATION),
            Some(&"/native_app_signin_succeeded".parse().unwrap())
        );

        let response = send(Method::GET, "/healthz").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

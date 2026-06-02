use std::env;

use axum_login::{
    AuthManagerLayerBuilder, login_required,
    tower_sessions::{CachingSessionStore, SessionManagerLayer, cookie::SameSite},
};
use openidconnect::{ClientId, ClientSecret, IssuerUrl, RedirectUrl};
use sqlx::PgPool;
use tower_sessions_moka_store::MokaStore;
use tower_sessions_sqlx_store::PostgresStore;

use crate::{
    users::Backend,
    web::{auth, oauth, protected},
};

pub struct App {
    db: PgPool,
}

impl App {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let db = PgPool::connect("postgres://%2Frun%2Fpostgresql/zed").await?;
        sqlx::migrate!().run(&db).await?;

        Ok(Self { db })
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        let moka_store = MokaStore::new(Some(4096));
        let db_store = PostgresStore::new(self.db.clone());
        db_store.migrate().await.unwrap();
        let session_store = CachingSessionStore::new(moka_store, db_store);
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_same_site(SameSite::Lax); // Ensure we send the cookie from the OAuth redirect.
        // TODO: really needed?

        let client_id = env::var("CLIENT_ID")
            .map(ClientId::new)
            .expect("CLIENT_ID should be provided.");
        let client_secret = env::var("CLIENT_SECRET").map(ClientSecret::new).ok();
        let issuer_url = env::var("ISSUER_URL")
            .map(IssuerUrl::new)
            .expect("ISSUER_URL should be provided")
            .expect("ISSUER_URL should be in a valid format");

        let redirect_url = env::var("REDIRECT_URL")
            .map(RedirectUrl::new)
            .expect("REDIRECT_URL should be provided")
            .expect("REDIRECT_URL should be a valid URL");

        let backend = Backend::new(self.db, client_id, client_secret, issuer_url, redirect_url);
        let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

        let app = protected::router()
            .route_layer(login_required!(Backend, login_url = "/login"))
            .merge(auth::router())
            .merge(oauth::router())
            .layer(auth_layer);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
            .await
            .unwrap();
        axum::serve(listener, app.into_make_service()).await?;

        Ok(())
    }
}

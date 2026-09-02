mod app;
mod assets;
mod config;
mod database;
mod oidc;
mod user_updates;

pub use app::serve;
pub use config::{Config, Secrets};

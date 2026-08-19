mod app;
mod assets;
mod config;
mod database;
mod oidc;

pub use app::serve;
pub use config::{Config, Secrets};

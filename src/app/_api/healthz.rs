use topcoat::Result;
use topcoat::router::{StatusCode, route};

#[route(GET)]
async fn health() -> Result<StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}

use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::{StatusCode, route};

topcoat::router::segment!(kind = CatchAll, rename = "path");

#[route(POST)]
async fn ignore_feedback(_cx: &Cx) -> Result<StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}

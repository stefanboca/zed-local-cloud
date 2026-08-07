use topcoat::{
    Result,
    context::Cx,
    router::{StatusCode, route},
};

use super::super::validate_cached_credentials;

topcoat::router::segment!(kind = CatchAll, rename = "path");

#[route(POST)]
async fn ignore_feedback(cx: &Cx) -> Result<StatusCode> {
    validate_cached_credentials(cx).await?;
    Ok(StatusCode::NO_CONTENT)
}

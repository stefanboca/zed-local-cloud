use topcoat::{
    Result,
    context::Cx,
    router::{StatusCode, route},
};

topcoat::router::segment!(kind = CatchAll, rename = "path");

#[route(POST)]
async fn ignore_feedback(_cx: &Cx) -> Result<StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}

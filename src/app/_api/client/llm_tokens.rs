use serde_json::{Value, json};
use topcoat::Result;
use topcoat::context::Cx;
use topcoat::router::content::Json;
use topcoat::router::route;

use super::validate_cached_credentials;

topcoat::router::segment!(rename = "llm_tokens");

#[route(POST)]
async fn llm_tokens(cx: &Cx) -> Result<Json<Value>> {
    validate_cached_credentials(cx).await?;
    Ok(Json(json!({"token": "local-ai-disabled"})))
}

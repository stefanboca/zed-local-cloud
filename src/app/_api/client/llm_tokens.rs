use serde_json::{Value, json};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, route},
};

use super::validate_cached_credentials;

topcoat::router::segment!(rename = "llm_tokens");

#[route(POST)]
async fn llm_tokens(cx: &Cx) -> Result<Json<Value>> {
    validate_cached_credentials(cx).await?;
    Ok(Json(json!({"token": "local-ai-disabled"})))
}

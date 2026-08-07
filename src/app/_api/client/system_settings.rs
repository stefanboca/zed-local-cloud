use serde_json::{Value, json};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, route},
};

use super::validate_cached_credentials;

topcoat::router::segment!(rename = "system_settings");

#[route([PATCH, POST])]
async fn update_system_settings(cx: &Cx, Json(body): Json<Value>) -> Result<Json<Value>> {
    validate_cached_credentials(cx).await?;
    Ok(Json(json!({
        "selected_organization_id": body
            .get("selected_organization_id")
            .cloned()
            .unwrap_or(Value::Null)
    })))
}

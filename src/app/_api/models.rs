use serde_json::{Value, json};
use topcoat::Result;
use topcoat::router::content::Json;
use topcoat::router::route;

#[route(GET)]
async fn models() -> Result<Json<Value>> {
    Ok(Json(json!({
        "models": [],
        "default_model": null,
        "default_fast_model": null,
        "recommended_models": []
    })))
}

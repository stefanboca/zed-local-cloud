use serde_json::{Value, json};
use topcoat::{
    Result,
    router::{content::Json, route},
};

#[route(GET)]
async fn models() -> Result<Json<Value>> {
    Ok(Json(json!({
        "models": [],
        "default_model": null,
        "default_fast_model": null,
        "recommended_models": []
    })))
}

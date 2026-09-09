use anyhow::Context as _;
use serde_json::{Value, json};
use topcoat::Result;
use topcoat::context::{Cx, app_context};
use topcoat::router::content::Json;
use topcoat::router::error::{internal_server_error, unauthorized};
use topcoat::router::request::headers;
use topcoat::router::route;

use super::super::parse_authorization_header;
use crate::database::{self, Database};

#[route(GET)]
async fn authenticated_user(cx: &Cx) -> Result<Json<Value>> {
    let database = app_context::<Database>(cx);
    let credentials = parse_authorization_header(headers(cx))?;
    let user = database
        .authenticated_user(credentials.user_id, &credentials.access_token)
        .await
        .context("failed to load the authenticated user")
        .map_err(internal_server_error)?
        .ok_or_else(unauthorized)?;

    Ok(Json(authenticated_user_response(&user)))
}

fn authenticated_user_response(user: &database::AuthenticatedLocalUser) -> Value {
    json!({
        "user": {
            "id_v2": user.stable_id,
            "legacy_user_id": user.user_id,
            "metrics_id": user.metrics_id.to_string(),
            "username": user.username,
            "avatar_url": user.avatar_url,
            "github_login": user.github_login,
            "name": user.name,
            "is_staff": user.admin,
            "accepted_tos_at": null,
            "has_connected_to_collab_once": user.connected_once
        },
        "feature_flags": [],
        "organizations": [{"id": "local", "name": "Local", "is_personal": true}],
        "default_organization_id": "local",
        "plans_by_organization": {"local": "zed_free"},
        "configuration_by_organization": {
            "local": {
                "is_zed_model_provider_enabled": false,
                "is_agent_thread_feedback_enabled": false,
                "is_collaboration_enabled": true,
                "edit_prediction": {
                    "is_enabled": false,
                    "is_feedback_enabled": false
                }
            }
        },
        "plan": {
            "plan_v3": "zed_free",
            "subscription_period": null,
            "usage": {
                "edit_predictions": {"used": 0, "limit": "unlimited"}
            },
            "trial_started_at": null,
            "is_account_too_young": false,
            "has_overdue_invoices": false
        }
    })
}

use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, error::internal_server_error, headers, route},
};

use super::super::validate_internal_api_key;
use crate::{
    app::production_state,
    store::{self, LocalUser},
};

topcoat::router::segment!(rename = "fuzzy_search");

#[derive(Deserialize)]
struct FuzzySearchChannelMembersBody {
    channel_id: i32,
    query: String,
    limit: u32,
}

#[derive(Serialize)]
struct ChannelMembersResponse {
    channel_members: Vec<ChannelMember>,
    users: Vec<LocalUser>,
}

#[derive(Serialize)]
struct ChannelMember {
    legacy_user_id: i32,
    kind: &'static str,
    role: String,
}

#[route(POST)]
async fn fuzzy_search_channel_members(
    cx: &Cx,
    Json(body): Json<FuzzySearchChannelMembersBody>,
) -> Result<Json<ChannelMembersResponse>> {
    let state = production_state(cx)?;
    validate_internal_api_key(state, headers(cx))?;
    let rows = store::fuzzy_search_channel_members(
        &state.database,
        body.channel_id,
        &body.query,
        i64::from(body.limit.min(100)),
    )
    .await
    .map_err(|error| internal("failed to search channel members", error))?;

    let mut channel_members = Vec::with_capacity(rows.len());
    let mut users = Vec::with_capacity(rows.len());
    for row in rows {
        channel_members.push(ChannelMember {
            legacy_user_id: row.user_id,
            kind: if row.accepted { "member" } else { "invitee" },
            role: row.role.clone(),
        });
        users.push(LocalUser {
            user_id: row.user_id,
            stable_id: row.stable_id,
            username: row.username,
            github_login: row.github_login,
            avatar_url: row.avatar_url,
            name: row.name,
            admin: row.admin,
            connected_once: row.connected_once,
        });
    }

    Ok(Json(ChannelMembersResponse {
        channel_members,
        users,
    }))
}

fn internal(context: &'static str, error: impl Into<anyhow::Error>) -> topcoat::Error {
    let error = error.into().context(context);
    tracing::error!(%error, "request failed");
    internal_server_error(error).into()
}

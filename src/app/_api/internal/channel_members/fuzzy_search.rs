use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use topcoat::Result;
use topcoat::context::{Cx, app_context};
use topcoat::router::content::Json;
use topcoat::router::error::internal_server_error;
use topcoat::router::route;

use super::super::require_internal_api;
use crate::database::{Database, LocalUser};

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
    require_internal_api(cx)?;
    let rows = app_context::<Database>(cx)
        .fuzzy_search_channel_members(body.channel_id, &body.query, i64::from(body.limit.min(100)))
        .await
        .context("failed to search channel members")
        .map_err(internal_server_error)?;

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

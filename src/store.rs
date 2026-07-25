use serde::Serialize;
use sha2::{Digest as _, Sha256};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug)]
pub struct AuthenticatedProfile {
    pub user_id: i32,
    pub github_login: String,
    pub avatar_url: String,
    pub name: Option<String>,
    pub email_address: Option<String>,
    pub metrics_id: Uuid,
    pub upstream_is_staff: bool,
    pub upstream_connected_once: bool,
}

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct LocalUser {
    #[serde(rename = "legacy_user_id")]
    pub user_id: i32,
    #[serde(rename = "id")]
    pub stable_id: String,
    pub github_login: String,
    pub avatar_url: String,
    pub name: Option<String>,
    pub admin: bool,
    #[serde(rename = "connected_once")]
    pub connected_once: bool,
}

#[derive(Debug, FromRow)]
pub struct ChannelMemberUser {
    pub user_id: i32,
    pub stable_id: String,
    pub github_login: String,
    pub avatar_url: String,
    pub name: Option<String>,
    pub admin: bool,
    pub connected_once: bool,
    pub accepted: bool,
    pub role: String,
}

pub async fn sync_authenticated_user(
    database: &PgPool,
    profile: &AuthenticatedProfile,
    access_token: &str,
    admin: bool,
) -> sqlx::Result<bool> {
    let mut transaction = database.begin().await?;

    let connected_once = sqlx::query_scalar::<_, bool>(
        r#"
        INSERT INTO users (id, admin, connected_once)
        VALUES ($1, $2, $3)
        ON CONFLICT (id) DO UPDATE
        SET admin = EXCLUDED.admin
        RETURNING connected_once
        "#,
    )
    .bind(profile.user_id)
    .bind(admin)
    .bind(profile.upstream_connected_once)
    .fetch_one(&mut *transaction)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO local_user_profiles (
            user_id,
            github_login,
            avatar_url,
            name,
            email_address,
            metrics_id,
            upstream_is_staff
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (user_id) DO UPDATE
        SET github_login = EXCLUDED.github_login,
            avatar_url = EXCLUDED.avatar_url,
            name = EXCLUDED.name,
            email_address = EXCLUDED.email_address,
            metrics_id = EXCLUDED.metrics_id,
            upstream_is_staff = EXCLUDED.upstream_is_staff,
            updated_at = now()
        "#,
    )
    .bind(profile.user_id)
    .bind(&profile.github_login)
    .bind(&profile.avatar_url)
    .bind(&profile.name)
    .bind(&profile.email_address)
    .bind(profile.metrics_id)
    .bind(profile.upstream_is_staff)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO local_user_tokens (user_id, access_token_hash)
        VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE
        SET access_token_hash = EXCLUDED.access_token_hash,
            last_validated_at = now()
        "#,
    )
    .bind(profile.user_id)
    .bind(hash_access_token(access_token))
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(connected_once)
}

pub async fn cached_token_is_valid(
    database: &PgPool,
    user_id: i32,
    access_token: &str,
) -> sqlx::Result<bool> {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM local_user_tokens
            WHERE user_id = $1 AND access_token_hash = $2
        )
        "#,
    )
    .bind(user_id)
    .bind(hash_access_token(access_token))
    .fetch_one(database)
    .await
}

pub async fn users_by_ids(database: &PgPool, user_ids: &[i32]) -> sqlx::Result<Vec<LocalUser>> {
    sqlx::query_as(
        r#"
        SELECT
            profiles.user_id,
            'local-' || profiles.user_id::text AS stable_id,
            profiles.github_login,
            profiles.avatar_url,
            profiles.name,
            users.admin,
            users.connected_once
        FROM unnest($1::integer[]) WITH ORDINALITY AS requested(user_id, position)
        JOIN local_user_profiles profiles ON profiles.user_id = requested.user_id
        JOIN users ON users.id = profiles.user_id
        ORDER BY requested.position
        "#,
    )
    .bind(user_ids)
    .fetch_all(database)
    .await
}

pub async fn user_by_github_login(
    database: &PgPool,
    github_login: &str,
) -> sqlx::Result<Option<LocalUser>> {
    sqlx::query_as(
        r#"
        SELECT
            profiles.user_id,
            'local-' || profiles.user_id::text AS stable_id,
            profiles.github_login,
            profiles.avatar_url,
            profiles.name,
            users.admin,
            users.connected_once
        FROM local_user_profiles profiles
        JOIN users ON users.id = profiles.user_id
        WHERE lower(profiles.github_login) = lower($1)
        "#,
    )
    .bind(github_login)
    .fetch_optional(database)
    .await
}

pub async fn fuzzy_search_users(
    database: &PgPool,
    query: &str,
    limit: i64,
) -> sqlx::Result<Vec<LocalUser>> {
    sqlx::query_as(
        r#"
        SELECT
            profiles.user_id,
            'local-' || profiles.user_id::text AS stable_id,
            profiles.github_login,
            profiles.avatar_url,
            profiles.name,
            users.admin,
            users.connected_once
        FROM local_user_profiles profiles
        JOIN users ON users.id = profiles.user_id
        WHERE strpos(lower(profiles.github_login), lower($1)) > 0
           OR strpos(lower(coalesce(profiles.name, '')), lower($1)) > 0
        ORDER BY
            lower(profiles.github_login) = lower($1) DESC,
            left(lower(profiles.github_login), length($1)) = lower($1) DESC,
            length(profiles.github_login),
            lower(profiles.github_login)
        LIMIT $2
        "#,
    )
    .bind(query)
    .bind(limit)
    .fetch_all(database)
    .await
}

pub async fn fuzzy_search_channel_members(
    database: &PgPool,
    channel_id: i32,
    query: &str,
    limit: i64,
) -> sqlx::Result<Vec<ChannelMemberUser>> {
    sqlx::query_as(
        r#"
        SELECT
            profiles.user_id,
            'local-' || profiles.user_id::text AS stable_id,
            profiles.github_login,
            profiles.avatar_url,
            profiles.name,
            users.admin,
            users.connected_once,
            channel_members.accepted,
            channel_members.role
        FROM channel_members
        JOIN local_user_profiles profiles ON profiles.user_id = channel_members.user_id
        JOIN users ON users.id = profiles.user_id
        WHERE channel_members.channel_id = $1
          AND (
              strpos(lower(profiles.github_login), lower($2)) > 0
              OR strpos(lower(coalesce(profiles.name, '')), lower($2)) > 0
          )
        ORDER BY
            lower(profiles.github_login) = lower($2) DESC,
            left(lower(profiles.github_login), length($2)) = lower($2) DESC,
            length(profiles.github_login),
            lower(profiles.github_login)
        LIMIT $3
        "#,
    )
    .bind(channel_id)
    .bind(query)
    .bind(limit)
    .fetch_all(database)
    .await
}

pub fn hash_access_token(access_token: &str) -> String {
    format!("{:x}", Sha256::digest(access_token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_does_not_contain_the_token() {
        let hash = hash_access_token("secret-token");
        assert_eq!(hash.len(), 64);
        assert!(!hash.contains("secret-token"));
    }
}

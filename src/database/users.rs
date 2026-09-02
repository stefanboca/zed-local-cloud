use serde::Serialize;
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use super::Database;
use crate::oidc::OidcProfile;

#[derive(Debug)]
pub struct AuthenticatedLocalUser {
    pub user_id: i32,
    pub stable_id: String,
    pub username: String,
    pub github_login: String,
    pub avatar_url: String,
    pub name: Option<String>,
    pub metrics_id: Uuid,
    pub admin: bool,
    pub connected_once: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalUser {
    #[serde(rename = "legacy_user_id")]
    pub user_id: i32,
    #[serde(rename = "id")]
    pub stable_id: String,
    pub username: String,
    pub github_login: String,
    pub avatar_url: String,
    pub name: Option<String>,
    pub admin: bool,
    #[serde(rename = "connected_once")]
    pub connected_once: bool,
}

#[derive(Debug)]
pub struct ChannelMemberUser {
    pub user_id: i32,
    pub stable_id: String,
    pub username: String,
    pub github_login: String,
    pub avatar_url: String,
    pub name: Option<String>,
    pub admin: bool,
    pub connected_once: bool,
    pub accepted: bool,
    pub role: String,
}

impl Database {
    pub async fn sync_oidc_user(
        &self,
        profile: &OidcProfile,
        access_token: &str,
        admin: bool,
    ) -> sqlx::Result<i32> {
        let mut transaction = self.0.begin().await?;

        let identity_lock = oidc_identity_lock_key(&profile.issuer, &profile.subject);
        sqlx::query_file!("queries/user/lock_oidc_identity.sql", identity_lock)
            .execute(&mut *transaction)
            .await?;

        let existing_user_id = sqlx::query_file_scalar!(
            "queries/user/find_oidc_identity.sql",
            &profile.issuer,
            &profile.subject,
        )
        .fetch_optional(&mut *transaction)
        .await?;

        let user_id = if let Some(user_id) = existing_user_id {
            sqlx::query_file!(
                "queries/user/update_user.sql",
                user_id,
                admin,
                profile.email_address.as_deref(),
                profile.name.as_deref(),
            )
            .execute(&mut *transaction)
            .await?;

            sqlx::query_file!(
                "queries/user/update_profile.sql",
                user_id,
                &profile.avatar_url,
                profile.name.as_deref(),
                profile.email_address.as_deref(),
            )
            .execute(&mut *transaction)
            .await?;

            sqlx::query_file!(
                "queries/user/touch_oidc_identity.sql",
                &profile.issuer,
                &profile.subject,
            )
            .execute(&mut *transaction)
            .await?;
            user_id
        } else {
            let inserted = sqlx::query_file!(
                "queries/user/insert_user.sql",
                admin,
                profile.email_address.as_deref(),
                profile.name.as_deref(),
            )
            .fetch_one(&mut *transaction)
            .await?;
            let (user_id, metrics_id) = (inserted.id, inserted.metrics_id);

            let username = normalized_username(
                profile
                    .preferred_username
                    .as_deref()
                    .or_else(|| {
                        profile
                            .email_address
                            .as_deref()
                            .and_then(|email| email.split('@').next())
                    })
                    .unwrap_or("user"),
            );
            sqlx::query_file!("queries/user/lock_username.sql", &username)
                .execute(&mut *transaction)
                .await?;
            let username_taken =
                sqlx::query_file_scalar!("queries/user/username_exists.sql", &username)
                    .fetch_one(&mut *transaction)
                    .await?;
            let username = if username_taken {
                format!("{username}-{user_id}")
            } else {
                username
            };

            sqlx::query_file!(
                "queries/user/insert_profile.sql",
                user_id,
                username,
                &profile.avatar_url,
                profile.name.as_deref(),
                profile.email_address.as_deref(),
                metrics_id,
            )
            .execute(&mut *transaction)
            .await?;

            sqlx::query_file!(
                "queries/user/insert_oidc_identity.sql",
                &profile.issuer,
                &profile.subject,
                user_id,
            )
            .execute(&mut *transaction)
            .await?;
            user_id
        };

        sqlx::query_file!(
            "queries/user/insert_token.sql",
            user_id,
            hash_access_token(access_token),
        )
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(user_id)
    }

    pub async fn authenticated_user(
        &self,
        user_id: i32,
        access_token: &str,
    ) -> sqlx::Result<Option<AuthenticatedLocalUser>> {
        sqlx::query_file_as!(
            AuthenticatedLocalUser,
            "queries/user/authenticated.sql",
            user_id,
            hash_access_token(access_token),
        )
        .fetch_optional(&self.0)
        .await
    }

    pub async fn cached_token_is_valid(
        &self,
        user_id: i32,
        access_token: &str,
    ) -> sqlx::Result<bool> {
        sqlx::query_file_scalar!(
            "queries/user/cached_token_is_valid.sql",
            user_id,
            hash_access_token(access_token)
        )
        .fetch_one(&self.0)
        .await
    }

    pub async fn users_by_ids(&self, user_ids: &[i32]) -> sqlx::Result<Vec<LocalUser>> {
        sqlx::query_file_as!(LocalUser, "queries/user/by_ids.sql", user_ids,)
            .fetch_all(&self.0)
            .await
    }

    pub async fn user_by_github_login(
        &self,
        github_login: &str,
    ) -> sqlx::Result<Option<LocalUser>> {
        sqlx::query_file_as!(LocalUser, "queries/user/by_github_login.sql", github_login,)
            .fetch_optional(&self.0)
            .await
    }

    pub async fn fuzzy_search_users(
        &self,
        query: &str,
        limit: i64,
    ) -> sqlx::Result<Vec<LocalUser>> {
        sqlx::query_file_as!(LocalUser, "queries/user/fuzzy_search.sql", query, limit,)
            .fetch_all(&self.0)
            .await
    }

    pub async fn fuzzy_search_channel_members(
        &self,
        channel_id: i32,
        query: &str,
        limit: i64,
    ) -> sqlx::Result<Vec<ChannelMemberUser>> {
        sqlx::query_file_as!(
            ChannelMemberUser,
            "queries/user/fuzzy_search_channel_members.sql",
            channel_id,
            query,
            limit,
        )
        .fetch_all(&self.0)
        .await
    }
}

pub fn hash_access_token(access_token: &str) -> String {
    Sha256::digest(access_token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn oidc_identity_lock_key(issuer: &str, subject: &str) -> String {
    let mut digest = Sha256::new();
    digest.update((issuer.len() as u64).to_be_bytes());
    digest.update(issuer.as_bytes());
    digest.update(subject.as_bytes());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn normalized_username(value: &str) -> String {
    let mut username = String::with_capacity(value.len().min(48));
    for character in value.trim().chars() {
        if username.len() >= 48 {
            break;
        }
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            username.push(character);
        } else if !username.ends_with('-') {
            username.push('-');
        }
    }
    let username = username.trim_matches(['-', '.']);
    if username.is_empty() {
        "user".to_owned()
    } else {
        username.to_owned()
    }
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

    #[test]
    fn normalizes_oidc_usernames_for_zed() {
        assert_eq!(normalized_username(" Ada Lovelace "), "Ada-Lovelace");
        assert_eq!(normalized_username("!!!"), "user");
    }

    #[test]
    fn oidc_identity_lock_key_is_safe_and_unambiguous() {
        let key = oidc_identity_lock_key("https://auth.example.com", "alice");
        assert_eq!(key.len(), 64);
        assert!(!key.contains('\0'));
        assert_ne!(
            oidc_identity_lock_key("https://auth.example.com/a", "lice"),
            oidc_identity_lock_key("https://auth.example.com", "a/lice")
        );
    }
}

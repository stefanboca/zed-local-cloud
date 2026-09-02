WITH
    existing_identity AS MATERIALIZED (
        SELECT user_id
        FROM local_oidc_identities
        WHERE issuer = $1 AND subject = $2
    ),

    synced_account AS (
        UPDATE "users" AS u
        SET
            "admin" = $3,
            email_address = $4,
            "name" = $5
        FROM existing_identity
        WHERE u.id = existing_identity.user_id
    ),

    updated_profile AS (
        UPDATE local_user_profiles
        SET
            avatar_url = $6,
            "name" = $5,
            email_address = $4,
            upstream_is_staff = false,
            updated_at = now()
        FROM existing_identity
        WHERE local_user_profiles.user_id = existing_identity.user_id
    ),

    updated_identity AS (
        UPDATE local_oidc_identities
        SET updated_at = now()
        FROM existing_identity
        WHERE
            local_oidc_identities.issuer = $1
            AND local_oidc_identities.subject = $2
    ),

    inserted_token AS (
        INSERT INTO local_user_tokens (user_id, access_token_hash)
        SELECT
            existing_identity.user_id,
            $7 AS access_token_hash
        FROM existing_identity
    )

SELECT existing_identity.user_id
FROM existing_identity

WITH
    inserted_account AS (
        INSERT INTO "users" ("admin", email_address, connected_once, "name")
        VALUES ($1, $2, false, $3)
        RETURNING id, metrics_id
    ),

    inserted_profile AS (
        INSERT INTO local_user_profiles (
            user_id,
            github_login,
            avatar_url,
            "name",
            email_address,
            metrics_id,
            upstream_is_staff
        )
        SELECT
            inserted_account.id AS user_id,
            CASE
                WHEN
                    EXISTS (
                        SELECT 1
                        FROM local_user_profiles AS profiles
                        WHERE lower(profiles.github_login) = lower($4)
                    )
                    THEN $4 || '-' || inserted_account.id::text
                ELSE $4
            END AS github_login,
            $5 AS avatar_url,
            $3 AS "name",
            $2 AS email_address,
            inserted_account.metrics_id,
            false AS upstream_is_staff
        FROM inserted_account
        RETURNING user_id
    ),

    inserted_identity AS (
        INSERT INTO local_oidc_identities (issuer, subject, user_id)
        SELECT
            $6 AS issuer,
            $7 AS subject,
            inserted_profile.user_id
        FROM inserted_profile
        RETURNING user_id
    ),

    inserted_token AS (
        INSERT INTO local_user_tokens (user_id, access_token_hash)
        SELECT
            inserted_identity.user_id,
            $8 AS access_token_hash
        FROM inserted_identity
        RETURNING user_id
    )

SELECT inserted_token.user_id
FROM inserted_token

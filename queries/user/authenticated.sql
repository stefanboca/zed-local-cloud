SELECT
    profiles.user_id,
    'local-' || profiles.user_id::text AS "stable_id!",
    profiles.github_login AS "username!",
    profiles.github_login,
    profiles.avatar_url,
    profiles.name,
    profiles.metrics_id,
    users.admin,
    users.connected_once
FROM local_user_tokens tokens
JOIN local_user_profiles profiles ON profiles.user_id = tokens.user_id
JOIN users ON users.id = profiles.user_id
WHERE tokens.user_id = $1 AND tokens.access_token_hash = $2

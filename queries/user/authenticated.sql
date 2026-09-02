SELECT
    profiles.user_id,
    profiles.github_login AS "username!",
    profiles.github_login,
    profiles.avatar_url,
    profiles.name,
    profiles.metrics_id,
    users.admin,
    users.connected_once,
    'local-' || profiles.user_id::text AS "stable_id!"
FROM local_user_tokens AS tokens
    INNER JOIN
        local_user_profiles AS profiles
        ON tokens.user_id = profiles.user_id
    INNER JOIN users ON profiles.user_id = users.id
WHERE tokens.user_id = $1 AND tokens.access_token_hash = $2

SELECT
    profiles.user_id,
    profiles.github_login AS "username!",
    profiles.github_login,
    profiles.avatar_url,
    profiles.name,
    users.admin,
    users.connected_once,
    'local-' || profiles.user_id::text AS "stable_id!"
FROM unnest($1::integer []) WITH ORDINALITY AS requested (user_id, ordinal)
    INNER JOIN
        local_user_profiles AS profiles
        ON requested.user_id = profiles.user_id
    INNER JOIN users ON profiles.user_id = users.id
ORDER BY requested.ordinal

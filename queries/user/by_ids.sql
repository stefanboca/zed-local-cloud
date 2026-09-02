SELECT
    profiles.user_id,
    'local-' || profiles.user_id::text AS "stable_id!",
    profiles.github_login AS "username!",
    profiles.github_login,
    profiles.avatar_url,
    profiles.name,
    users.admin,
    users.connected_once
FROM unnest($1::integer[]) WITH ORDINALITY AS requested(user_id, position)
JOIN local_user_profiles profiles ON profiles.user_id = requested.user_id
JOIN users ON users.id = profiles.user_id
ORDER BY requested.position

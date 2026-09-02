SELECT
    profiles.user_id,
    profiles.github_login AS "username!",
    profiles.github_login,
    profiles.avatar_url,
    profiles.name,
    users.admin,
    users.connected_once,
    'local-' || profiles.user_id::text AS "stable_id!"
FROM local_user_profiles AS profiles
    INNER JOIN users ON profiles.user_id = users.id
WHERE
    lower(profiles.github_login) % lower($1)
    OR lower($1) <% lower(coalesce(profiles.name, ''))
ORDER BY
    greatest(
        similarity(lower(profiles.github_login), lower($1)),
        word_similarity(lower($1), lower(coalesce(profiles.name, '')))
    ) DESC
LIMIT $2

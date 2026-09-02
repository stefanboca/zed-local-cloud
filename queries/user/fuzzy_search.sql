SELECT
    profiles.user_id,
    'local-' || profiles.user_id::text AS "stable_id!",
    profiles.github_login AS "username!",
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

SELECT
    profiles.user_id,
    profiles.github_login AS "username!",
    profiles.github_login,
    profiles.avatar_url,
    profiles.name,
    users.admin,
    users.connected_once,
    channel_members.accepted,
    channel_members.role,
    'local-' || profiles.user_id::text AS "stable_id!"
FROM channel_members
    INNER JOIN
        local_user_profiles AS profiles
        ON channel_members.user_id = profiles.user_id
    INNER JOIN users ON profiles.user_id = users.id
WHERE
    channel_members.channel_id = $1
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

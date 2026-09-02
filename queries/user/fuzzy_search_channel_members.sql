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
        lower(profiles.github_login) % lower($2)
        OR lower($2) <% lower(coalesce(profiles.name, ''))
    )
ORDER BY
    greatest(
        similarity(lower(profiles.github_login), lower($2)),
        word_similarity(lower($2), lower(coalesce(profiles.name, '')))
    ) DESC
LIMIT $3

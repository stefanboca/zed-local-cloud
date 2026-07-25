CREATE TABLE local_user_profiles (
    user_id integer PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    github_login text NOT NULL,
    avatar_url text NOT NULL,
    name text,
    email_address text,
    metrics_id uuid NOT NULL,
    upstream_is_staff boolean NOT NULL DEFAULT false,
    updated_at timestamp without time zone NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX local_user_profiles_github_login_idx
    ON local_user_profiles (lower(github_login));

CREATE TABLE local_user_tokens (
    user_id integer PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    access_token_hash text NOT NULL,
    last_validated_at timestamp without time zone NOT NULL DEFAULT now()
);

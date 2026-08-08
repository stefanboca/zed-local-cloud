CREATE TABLE local_browser_sessions (
    token_hash bytea PRIMARY KEY,
    user_id integer NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp without time zone NOT NULL DEFAULT now(),
    updated_at timestamp without time zone NOT NULL DEFAULT now(),
    CHECK (octet_length(token_hash) = 32)
);

CREATE INDEX local_browser_sessions_expires_at_idx
    ON local_browser_sessions (expires_at);

CREATE TABLE local_oidc_identities (
    issuer text NOT NULL,
    subject text NOT NULL,
    user_id integer NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    created_at timestamp without time zone NOT NULL DEFAULT now(),
    updated_at timestamp without time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (issuer, subject)
);

CREATE TABLE local_login_sessions (
    token_hash bytea PRIMARY KEY,
    native_app_port integer NOT NULL CHECK (
        native_app_port BETWEEN 1 AND 65535
    ),
    native_app_public_key text NOT NULL,
    oidc_state_hash bytea,
    oidc_nonce text,
    oidc_pkce_verifier text,
    expires_at_epoch bigint NOT NULL,
    created_at timestamp without time zone NOT NULL DEFAULT now(),
    CHECK (octet_length(token_hash) = 32),
    CHECK (
        oidc_state_hash IS NULL
        OR octet_length(oidc_state_hash) = 32
    )
);

CREATE INDEX local_login_sessions_expires_at_idx
    ON local_login_sessions (expires_at_epoch);

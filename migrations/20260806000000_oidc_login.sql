CREATE TABLE local_oidc_identities (
    issuer text NOT NULL,
    subject text NOT NULL,
    user_id integer NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    created_at timestamp without time zone NOT NULL DEFAULT now(),
    updated_at timestamp without time zone NOT NULL DEFAULT now(),
    PRIMARY KEY (issuer, subject)
);

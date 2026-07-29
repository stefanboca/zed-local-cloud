# Generated service credentials

## Goal

Remove operator-managed secrets from the NixOS module. The PostgreSQL URL is
ordinary module configuration, while the internal API key is generated once
on the target host and shared privately by the local Zed services.

## Module interface

Remove `services.zed-auth.secretsFile`.

Add `services.zed-auth.database.url` as a string option. Its default is the
current peer-authenticated local socket URL derived from the configured service
user and database name.

`services.zed-auth.database.createLocally` continues to control whether the
module enables and provisions PostgreSQL. It no longer controls whether the
setup unit exists or whether collab migrations are run.

## Setup and persistent state

Rename `zed-collab-database-migrate.service` to `zed-setup.service`.
`zed-setup` always runs before `zed-auth` and `zed-collab`. It has two ordered
responsibilities:

1. Create `/var/lib/zed/zed-auth.toml` when that file is absent or empty. The
   file contains only `ZED_CLOUD_INTERNAL_API_KEY`, generated from 128 random
   bytes with `openssl rand -base64 128` and stored without embedded newlines.
   An existing non-empty file is never replaced.
2. Apply the packaged collab schema with `sqlx migrate` using the configured
   database URL.

The setup, auth, and collab units all run as the configured system user/group
and declare `StateDirectory = "zed"` with mode `0700`. The generated TOML file
is created with mode `0600`.

Unit settings refer to the state root with systemd's `%S` specifier, while
shell scripts use the corresponding `$STATE_DIRECTORY` environment variable.
Collab's working directory uses `%t/zed-collab`.

When local database management is enabled, `zed-setup` requires and starts
after `postgresql.target`. With an external database, the setup unit still
generates the key and attempts migrations but has no PostgreSQL unit
dependency.

## Runtime configuration flow

The database URL is passed directly to:

- `zed-setup` for `sqlx migrate`;
- `zed-auth` through `DATABASE_URL` in its service environment; and
- `zed-collab` through its non-secret `.env.toml`.

Percent signs in the two systemd environment assignments are doubled while
rendering the units so URL-encoded socket paths survive systemd specifier
expansion. The module option and collab TOML retain the ordinary URL.

Both long-running services read the generated file directly from the shared
state directory. The facade receives
`ZED_AUTH_SECRETS_FILE=/var/lib/zed/zed-auth.toml`; collab links that file to
`.env.secret.toml`. `LoadCredential` is unnecessary because all three units
already run as the same user and can access the same mode-`0700` state
directory.

If key generation or database migration fails, `zed-setup` fails and neither
long-running service starts. Caddy only wants the long-running services and is
not prevented from starting by their failure.

## Validation

The NixOS VM test will configure the module without a secrets file and verify:

- `zed-setup`, auth, collab, and Caddy become healthy;
- all three Zed units use the configured user and shared state directory;
- the state directory and generated key file have restrictive ownership and
  permissions;
- the generated TOML contains a non-empty base64 internal key and no database
  URL;
- auth and collab read the same generated secret file;
- restarting `zed-setup` preserves the existing key;
- the database URL appears in ordinary auth and collab configuration;
- collab and facade migrations still run in the required order; and
- the existing HTTP and RPC health checks continue to pass.

The standalone VM configuration and README examples will no longer construct
or request an operator-supplied secret file.

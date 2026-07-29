# Zed local auth facade

This service lets an unmodified Zed client authenticate with upstream Zed while
using a local `crates/collab` server. It owns the public Zed-compatible routes,
synchronizes authenticated users into collab's Postgres database, implements
collab's internal user APIs locally, and leaves generic reverse proxying to
Caddy.

The facade listens internally on `http://localhost:8787` by default. That is
also the Cloud URL used by an unmodified development-mode collab server. Caddy
provides the public HTTPS endpoint and routes RPC, extension, Cloud, and web
traffic to the appropriate backend. Browser sign-in is delegated to upstream
Zed.

## Zed compatibility baseline

The current packaged compatibility baseline is
[zed-industries/zed@952d712dac48a4af2c54fb22c82d82a9d69b72d4](https://github.com/zed-industries/zed/commit/952d712dac48a4af2c54fb22c82d82a9d69b72d4),
the `v1.11.3` tag from July 15, 2026.

That revision is the reference baseline for:

- native app sign-in query and callback behavior
- `GetAuthenticatedUserResponse` JSON fields
- collab's internal user API request and response types
- RPC and Cloud WebSocket headers
- collab's Postgres schema

Recheck these contracts when updating to a newer Zed revision.

## Runtime architecture

- Postgres: `127.0.0.1:5432`, shared by collab and the facade
- Local collab: `127.0.0.1:8080`
- Local auth and internal API: `127.0.0.1:8787`
- Public endpoint: Caddy, configured by the NixOS module
- Zed setting: `ZED_SERVER_URL=https://zed.example.com`

Caddy redirects `/rpc` to an absolute `/actual_rpc` URL on the incoming
authority, then reverse proxies that WebSocket directly to collab's `/rpc` on
port `8080`. The Rust service does not implement a generic HTTP or WebSocket
proxy.

## Setup

### Database

Use a fresh `zed` database. Load the schema from the reference Zed checkout:

```sh
createdb -U postgres zed
psql -U postgres -d zed \
  -f /path/to/zed/crates/collab/migrations/20251208000000_test_schema.sql
```

The schema file is not idempotent. Skip these commands when the database is
already bootstrapped.

The facade and collab must use the same database and internal API key. Zed's
checked-in collab development configuration uses:

```text
DATABASE_URL=postgres://postgres@localhost/zed
ZED_CLOUD_INTERNAL_API_KEY=internal-api-key-secret
```

The facade applies its own `local_user_profiles` and `local_user_tokens`
migration automatically at startup.

### Start collab

Run collab inside Zed's upstream Nix development shell so it receives the
correct OpenSSL and native dependencies:

```sh
cd /path/to/zed
nix develop -c cargo run --package=collab -- serve collab
```

### Start the facade

```sh
cd /path/to/zed-auth
DATABASE_URL=postgres://postgres@localhost/zed cargo run
```

To grant local collab administrator privileges, optionally add:

```sh
LOCAL_ADMIN_GITHUB_LOGINS=your-github-login
```

### NixOS module

The flake exports the `zed-auth` and `zed-collab` packages, an overlay, and
`nixosModules.default`. The collab derivation uses
`inherit (zed-editor) src`, so its source revision follows the `zed-editor`
package in the flake's nixpkgs input. Add both the overlay and module to your
NixOS configuration:

```nix
{
  inputs.zed-auth.url = "github:your-name/zed-auth";

  outputs = {nixpkgs, zed-auth, ...}: {
    nixosConfigurations.your-host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        zed-auth.nixosModules.default
        {
          nixpkgs.overlays = [zed-auth.overlays.default];

          services.zed-auth = {
            enable = true;
            virtualHost = "zed.example.com";
            adminGithubLogins = ["your-github-login"];
          };
          services.caddy.openFirewall = true;
        }
      ];
    };
  };
}
```

For a complete local VM using the module, PostgreSQL, and Caddy:

```console
$ nix run .#vm
```

The VM forwards host `127.0.0.1:18787` to guest Caddy. Set Zed's
`server_url` to `http://localhost:18787`. The VM uses the module's normal
generated internal API key in its ephemeral state directory.

Run the NixOS integration test with:

```console
$ nix build .#checks.x86_64-linux.nixos-basic
```

The test is defined in `tests/nixos/default.nix`. It boots a clean VM and
checks database initialization, generated-key permissions and persistence,
migration ordering, service users and health, Caddy routing, and the upstream
sign-in completion redirect.

The module creates a local PostgreSQL database owned by the configured system
user and starts `zed-setup` before collab and the facade. All three units share
the mode-`0700` `zed` systemd state directory and run as the configured system
user/group. On its first run, `zed-setup` generates a 128-byte base64 internal
API key with OpenSSL in `/var/lib/zed/zed-auth.toml`; later runs preserve the
non-empty mode-`0600` file. It then applies the schema shipped with the
packaged collab server using `sqlx migrate`. Collab and the facade read the
same generated key directly from the shared state directory. The facade
applies its own embedded migrations before binding its listener.

The database URL is not secret. Configure it with
`services.zed-auth.database.url`; it defaults to the peer-authenticated local
socket URL `postgres://zed@%2Frun%2Fpostgresql/zed`. The module passes it to
setup, collab, and the facade as ordinary configuration.

SQLx records the collab schema migration in the shared database and skips it
after the first successful run. Collab and the facade use distinct migration
sets in the same SQLx ledger, so both migrators ignore versions owned by the
other set while still validating their own checksums. When local database
management is enabled,
`services.zed-auth.database.name` must match `services.zed-auth.user` so
PostgreSQL can grant ownership and use peer authentication without a password;
both default to `zed`. Set `services.zed-auth.database.createLocally = false`
when using an externally managed database; `zed-setup` still runs migrations
against `services.zed-auth.database.url`. The facade listens only on loopback.
The module enables Caddy and configures the complete public reverse proxy;
configure `services.caddy.openFirewall` as usual if Caddy should open ports 80
and 443.

The upstream collab migration is currently a full schema snapshot whose
numeric SQLx version has remained unchanged across Zed releases. If upstream
edits that file, SQLx rejects an existing database because its recorded
checksum no longer matches; it does not derive an incremental upgrade.
Therefore, before advancing this project's Zed baseline across a schema
change, add and test a project-owned incremental migration for existing
databases. A fresh database continues to use the packaged upstream snapshot.

### Start Zed

```sh
cd /path/to/zed
ZED_SERVER_URL=https://zed.example.com nix develop -c cargo run --package=zed
```

Sign in normally. The browser uses upstream `zed.dev`, while subsequent Cloud
and collaboration requests go through the local facade.

### Upstream service terms

Zed's open-source licenses govern the packaged software, but use of
Zed-operated authentication, API, extension, and web services is separately
subject to the [Zed Terms of Service](https://zed.dev/terms). This project is
unofficial, and Zed does not currently document this hybrid self-hosting setup
as a supported service-access mechanism. In particular, the Terms restrict
access to non-public service APIs and require use of Zed-designated access
mechanisms. Obtain Zed's confirmation before operating this for third parties.

This project does not proxy Zed-hosted LLM requests. It returns a local empty
model catalog, leaving Zed's supported direct-provider, gateway, and local
model integrations available to each client.

## Authentication and user synchronization

`GET /native_app_signin` preserves Zed's native app query parameters and
redirects the browser to upstream Zed. Upstream returns the RSA-encrypted access
token directly to the native app.

On `GET /client/users/me`, the facade:

1. Parses `Authorization: <user_id> <access_token>`.
2. Validates the unchanged credentials with upstream Cloud.
3. Verifies that the returned user ID matches the header.
4. Inserts or updates the user in collab's `users` table.
5. Preserves collab's existing `connected_once` value.
6. Inserts or updates `local_user_profiles`.
7. Stores only a SHA-256 token hash in `local_user_tokens`.
8. Returns a local organization with collaboration enabled and hosted AI
   disabled.

Upstream access tokens are never stored in plaintext.

`GET /client/users/connect` validates the cached credential hash and keeps the
Cloud WebSocket alive. It does not currently send account-change events.

## Data ownership and local administrators

Collab owns the `users` table and collaboration foreign keys. The facade adds:

- `local_user_profiles`: GitHub login, avatar, name, optional email, upstream
  metrics ID, and original upstream staff status
- `local_user_tokens`: SHA-256 access-token hash and validation timestamp

`LOCAL_ADMIN_GITHUB_LOGINS` is an optional comma-separated, case-insensitive
list. Matching users receive `users.admin = true`. The facade also maps this to
`user.is_staff` in its local response because collab derives its principal's
admin flag from that field. This does not change upstream Zed permissions.

Facade-owned tables are created by
`migrations/20260724000000_local_facade.sql`. The old prototype migration that
created its own incompatible `users` table has been removed; collab must apply
its schema first. Existing databases that recorded the old migration remain
usable because the facade migrator ignores migration versions owned by another
or removed migration set.

## Internal API key

`ZED_CLOUD_INTERNAL_API_KEY` is an arbitrary shared local secret. The NixOS
module generates it automatically in `/var/lib/zed/zed-auth.toml` and provides
the same value to collab and the facade. It is not issued by upstream Zed and
is never sent there.

For a standalone non-NixOS deployment, configure the same value for both
processes:

```sh
ZED_CLOUD_INTERNAL_API_KEY=your-local-secret
```

Collab uses it as a Bearer token when calling these locally implemented
endpoints:

- `POST /internal/users/look_up_by_legacy_id`
- `POST /internal/users/look_up_by_github_login`
- `POST /internal/users/fuzzy_search`
- `POST /internal/channel_members/fuzzy_search_by_github_login`

Only users who have authenticated through this facade appear in local lookup
and search results.

## Routes and Caddy behavior

The facade handles only:

- `GET /native_app_signin`: redirect to upstream native sign-in
- `GET /client/users/me`: upstream validation and local synchronization
- `GET /client/users/connect`: local keepalive WebSocket
- `PATCH` or `POST /client/system_settings`: local minimal response
- `POST /client/llm_tokens`: returns a non-secret local placeholder token after
  authenticating the user
- client feedback routes: return `204`

The module's Caddy virtual host uses named matchers to route:

- `/rpc`: temporary redirect to the absolute `/actual_rpc` URL for the
  incoming scheme and authority
- `/native_app_signin_succeeded`: redirect to upstream Zed's static
  confirmation page
- `/actual_rpc`: rewrite to `/rpc` and WebSocket proxy directly to collab
- `/models`: return an empty hosted-model catalog locally
- facade-owned routes: proxy to `127.0.0.1:8787`
- `/extensions*`: proxy to `https://api.zed.dev`
- remaining `/client/*`: proxy to upstream Cloud
- remaining paths: proxy to upstream web

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `BIND_ADDRESS` | `127.0.0.1:8787` | Facade listen address |
| `DATABASE_URL` | `postgres://zed@%2Frun%2Fpostgresql/zed` | Shared collab Postgres database |
| `UPSTREAM_WEB_URL` | `https://zed.dev` | Native sign-in origin |
| `UPSTREAM_CLOUD_URL` | `https://cloud.zed.dev` | Credential validation origin |
| `ZED_CLOUD_INTERNAL_API_KEY` | `internal-api-key-secret` | Shared local internal API secret |
| `LOCAL_ADMIN_GITHUB_LOGINS` | empty | Comma-separated local collab administrators |
| `RUST_LOG` | `zed_auth=info,tower_http=info,sqlx=warn` | Tracing filter |
| `ZED_AUTH_SECRETS_FILE` | unset | TOML secrets file for standalone deployments |

## Validation status

- Upstream browser handoff, callback, and complete authentication work against
  the packaged baseline above.
- The local collab RPC connection works through Caddy.
- Rust formatting and Clippy with warnings denied pass.
- The regular Rust tests pass.
- The opt-in auth/database integration test passes against Postgres initialized
  with Zed's actual collab schema.
- Both SQLx migrations apply to that schema.
- Runtime key generation and persistence, service health, authentication
  rejection, and native sign-in redirect checks pass.
- The Zed reference worktree remains unchanged.

The remaining manual acceptance work is a two-user collaboration check:

1. Confirm synchronized rows in `users`, `local_user_profiles`, and
   `local_user_tokens`.
2. Sign in with a second user.
3. Verify local user search, contacts, invitations, and collaboration.
4. Confirm search never includes upstream users who have not used this local
   instance.

## Known limitations

- `/client/users/connect` is keepalive-only, so some account UI may remain stale
  until reconnect or restart.
- Hosted Zed AI is intentionally disabled for the local organization.
- Extension installation through the Caddy proxy has not yet been manually
  tested.

# Zed local auth facade

This service lets an unmodified Zed client authenticate with upstream Zed while
using a local `crates/collab` server. It owns the public Zed-compatible routes,
synchronizes authenticated users into collab's Postgres database, implements
collab's internal user APIs locally, and proxies the collaboration WebSocket.

The facade listens on `http://localhost:8787` by default. That is also the Cloud
URL used by an unmodified development-mode collab server, so no Zed source
changes are required. Browser sign-in through upstream Zed has been manually
confirmed.

## Zed compatibility baseline

The implementation was developed against
[zed-industries/zed@35eaeb94a7e5e9dcd7384df4bd21f01c30a38a09](https://github.com/zed-industries/zed/commit/35eaeb94a7e5e9dcd7384df4bd21f01c30a38a09),
committed on June 25, 2026.

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
- Public facade and local Cloud API: `127.0.0.1:8787`
- Zed setting: `ZED_SERVER_URL=http://localhost:8787`

The facade's `/rpc` route redirects Zed to `/actual_rpc`. Its built-in
WebSocket proxy then connects to collab's `/rpc` on port `8080`.

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

### Start Zed

```sh
cd /path/to/zed
ZED_SERVER_URL=http://localhost:8787 nix develop -c cargo run --package=zed
```

Sign in normally. The browser uses upstream `zed.dev`, while subsequent Cloud
and collaboration requests go through the local facade.

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

The original prototype migration remains unchanged for SQLx checksum
compatibility. Facade-owned tables are created by
`migrations/20260724000000_local_facade.sql`.

## Internal API key

`ZED_CLOUD_INTERNAL_API_KEY` is an arbitrary shared local secret chosen by the
operator. Configure the same value for collab and the facade:

```sh
ZED_CLOUD_INTERNAL_API_KEY=your-local-secret
```

It is not issued by upstream Zed and is never sent there. Collab uses it as a
Bearer token when calling these locally implemented endpoints:

- `POST /internal/users/look_up_by_legacy_id`
- `POST /internal/users/look_up_by_github_login`
- `POST /internal/users/fuzzy_search`
- `POST /internal/channel_members/fuzzy_search_by_github_login`

Only users who have authenticated through this facade appear in local lookup
and search results.

## Routes and proxy behavior

- `GET /native_app_signin`: redirect to upstream native sign-in
- `GET /native_app_signin_succeeded`: local success page
- `GET /client/users/me`: upstream validation and local synchronization
- `GET /client/users/connect`: local keepalive WebSocket
- `PATCH` or `POST /client/system_settings`: local minimal response
- `POST /client/llm_tokens`: returns `403`
- client feedback routes: return `204`
- `GET /rpc`: redirect to the facade's `/actual_rpc`
- `GET /actual_rpc`: bidirectional WebSocket proxy to local collab
- `/extensions*`: proxy to `https://api.zed.dev`
- other `/client/*` routes: upstream Cloud fallback
- other routes: upstream web fallback

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `BIND_ADDRESS` | `127.0.0.1:8787` | Facade listen address |
| `DATABASE_URL` | `postgres://%2Frun%2Fpostgresql/zed` | Shared collab Postgres database |
| `PUBLIC_BASE_URL` | `http://localhost:8787` | URL returned by the `/rpc` redirect |
| `LOCAL_COLLAB_URL` | `http://127.0.0.1:8080` | Local collab HTTP origin |
| `UPSTREAM_WEB_URL` | `https://zed.dev` | Native sign-in and web fallback origin |
| `UPSTREAM_API_URL` | `https://api.zed.dev` | Extension API origin |
| `UPSTREAM_CLOUD_URL` | `https://cloud.zed.dev` | Credential validation and Cloud fallback origin |
| `ZED_CLOUD_INTERNAL_API_KEY` | `internal-api-key-secret` | Shared local internal API secret |
| `LOCAL_ADMIN_GITHUB_LOGINS` | empty | Comma-separated local collab administrators |
| `RUST_LOG` | `zed_auth=info,tower_http=info,sqlx=warn` | Tracing filter |

## Optional Caddy URL

To use the portless URL `http://zed.localhost`, proxy it to the facade:

```caddyfile
http://zed.localhost {
    reverse_proxy 127.0.0.1:8787
}
```

Run the facade with `PUBLIC_BASE_URL=http://zed.localhost`, then run Zed with
`ZED_SERVER_URL=http://zed.localhost`. Collab continues to reach the facade at
its existing `http://localhost:8787` development Cloud URL.

## Validation status

- Upstream browser sign-in works end to end.
- Rust formatting and Clippy with warnings denied pass.
- Nine regular tests pass.
- The WebSocket integration test verifies header and binary-frame forwarding.
- The opt-in auth/database integration test passes against Postgres initialized
  with Zed's actual collab schema.
- Both SQLx migrations apply to that schema.
- Runtime health, RPC redirect, authentication rejection, and native sign-in
  redirect checks pass.
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
- Extension installation through the proxy has not yet been manually tested.

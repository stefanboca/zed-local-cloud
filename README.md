# Zed local cloud

This project provides a local cloud service for an unmodified Zed client. It
also provides a local Zed collaboration server.

The local cloud uses OpenID Connect (OIDC) for user sign-in. It does not use a
Zed account. It does not send login data to Zed Cloud.

Yes, this design can operate without a login to a Zed server. The extension
catalog and extension downloads do not require a Zed login in the current Zed
baseline. Caddy sends only extension requests to `https://api.zed.dev`. All
other cloud and web requests stay on the local system. Caddy removes local
authorization data and cookies from extension requests.

The upstream extension service can change. Test extension installation after
each Zed update.

This README uses short sentences and controlled terms. It follows the main
writing rules of ASD-STE100 Simplified Technical English.

## Compatibility baseline

The project uses
[Zed v1.17.2](https://github.com/zed-industries/zed/tree/v1.17.2) as its
compatibility baseline. This is the Zed release packaged by the current
nixpkgs input.

The baseline defines these interfaces:

- The native application sign-in query and callback
- The `/client/users/me` response
- The collaboration internal user API
- The collaboration WebSocket headers
- The collaboration database schema

Check these interfaces before you update Zed.

## Components

The default installation has these components:

| Component                | Default address           | Function                                 |
| ------------------------ | ------------------------- | ---------------------------------------- |
| PostgreSQL               | Local socket              | Store collaboration and local cloud data |
| Zed collaboration server | `127.0.0.1:8080`          | Provide collaboration RPC                |
| Zed local cloud          | `127.0.0.1:8787`          | Provide login and cloud API routes       |
| Caddy                    | Public HTTPS address      | Route public requests                    |
| OIDC provider            | External or local address | Authenticate users                       |

The local cloud uses the [Topcoat](https://github.com/tokio-rs/topcoat) web
framework and encrypted private cookies for the OIDC transaction. It uses the
[openidconnect](https://docs.rs/openidconnect/4.0.1/openidconnect/) crate for
OIDC discovery, PKCE, token exchange, and token verification.

The local cloud and the collaboration server use the same PostgreSQL database.
They also use the same internal API key.

## Request flow

The sign-in flow is as follows:

1. Zed opens `/native_app_signin` in the browser.
2. The local cloud validates the Zed RSA public key.
3. The local cloud stores the OIDC state and typed RSA key in a private cookie.
4. The browser is redirected to the OIDC authorization code flow with PKCE.
5. The OIDC provider authenticates the user.
6. The local cloud verifies the OIDC state, nonce, ID token, and access-token hash.
7. The local cloud reads profile data from the ID token and the UserInfo endpoint.
8. The local cloud writes the user and administrator status to PostgreSQL.
9. The local cloud creates and encrypts a new local Zed access token.
10. The browser sends the encrypted token to the local Zed callback port.

The service stores only a SHA-256 hash of the local Zed access token. It does
not store the OIDC access token, OIDC client secret, or OIDC flow in PostgreSQL.

## OIDC claims

The local cloud uses the OIDC issuer and subject as the permanent user
identity. It reads these standard claims:

| Claim                | Use                                             |
| -------------------- | ----------------------------------------------- |
| `preferred_username` | Create the local username                       |
| `email`              | Supply an email address and a username fallback |
| `name`               | Supply the display name                         |
| `picture`            | Supply the Zed `avatar_url`                     |

The service reads claims from the verified ID token. If the provider has a
UserInfo endpoint, the service also reads claims from that endpoint. A
UserInfo value has priority over the same ID-token value.

For a new user, the service normalizes `preferred_username` and falls back to
the local part of `email`. Login fails if neither produces a usable username.

The `picture` claim is optional. If it is not present, `avatar_url` is an
empty string.

The administrator claim defaults to `groups`, which is requested through the
default `groups` scope. A string value or an array value is valid. Dot notation
selects a nested claim. For example,
`realm_access.roles` selects the `roles` member of `realm_access`.

The local cloud checks the administrator claim after each successful login.
It sets `users.admin` to `true` when the configured value is present. It sets
`users.admin` to `false` when the value is not present. The OIDC provider is
the source of the local administrator role.

## NixOS installation

The flake exports these items:

- The `zed-local-cloud` package for the local cloud
- The `zed-collab` package for the collaboration server
- The `zed-migrations` package for the database migrations
- The default NixOS module
- A package overlay

Use this example as a start:

```nix
{
  inputs.zed-local-cloud.url = "github:your-name/zed-local-cloud";

  outputs = {nixpkgs, zed-local-cloud, ...}: {
    nixosConfigurations.your-host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        zed-local-cloud.nixosModules.default
        {
          nixpkgs.overlays = [zed-local-cloud.overlays.default];

          services.zed-local-cloud = {
            enable = true;
            virtualHost = "zed.example.com";

            oidc = {
              issuerUrl = "https://auth.example.com";
              clientId = "zed-local";
              clientSecretFile = "/run/secrets/zed-oidc-client-secret";
              scopes = ["profile" "email" "groups"];
              adminClaim = "groups";
              adminValue = "zed-admin";
            };
          };

          services.caddy.openFirewall = true;
        }
      ];
    };
  };
}
```

The module copies `virtualHost` to the local cloud `base_url`. The local cloud
sets the scheme to `https` and adds `/auth/oidc/callback`. Register that
derived URL with the OIDC provider.

LiveKit is enabled by default through the upstream NixOS `services.livekit`
module. Caddy publishes it at `https://<virtualHost>/livekit`, and the collab
server receives that public URL plus the matching API key and secret. The
integration opens only LiveKit's configured UDP RTC port range; its TCP API
port remains behind Caddy. Configure ports, the RTC IP behavior, Redis, or the
package directly under `services.livekit`. Set
`services.zed-local-cloud.livekit.enable = false` to disable the whole
integration.

On first start, `zed-credentials` runs `livekit-server generate-keys`. It
stores the key and secret separately in `/var/lib/zed/livekit-api-key` and
`/var/lib/zed/livekit-api-secret` for collab. It also writes the pair to
`/var/lib/zed/livekit-keys` because that is the native `key: secret` format
expected by the upstream LiveKit module. Later runs retain the same pair.

To manage this credential yourself, set `services.livekit.keyFile` to a
runtime secret path containing one `key: secret` pair. During setup, the
integration splits that pair into the two private collab credential files.
LiveKit and collab receive their respective files through systemd credentials;
nothing is copied into the Nix store.

Systemd loads `clientSecretFile` as the `oidc-client-secret` credential. The
local cloud reads the raw secret from its private credential directory. The
TOML configuration does not contain the secret or its path. The service uses
a confidential OIDC client and requires this secret.

The module creates a local PostgreSQL database by default. It creates the
`zed` system user and the `zed` database. It uses PostgreSQL peer
authentication.

Set `services.zed-local-cloud.database.createLocally = false` for an external
database. Then set `services.zed-local-cloud.database.url` to the external
database URL.

The `zed-credentials` service creates a 64-byte cookie key and a 128-byte
internal API key on its first run. It stores them in `/var/lib/zed/cookie-key`
and `/var/lib/zed/internal-api-key`. Later runs keep the same non-empty files.
Their file mode is `0600`. The module loads them as the `cookie-key` and
`internal-api-key` systemd credentials. The generated TOML configuration does
not contain either key or path.

The cookie, OIDC, and internal API secrets each have a separate raw-value file.
Do not use TOML syntax in them. An externally managed LiveKit key file instead
uses LiveKit's `key: secret` format described above.

The local cloud applies all migrations in this project before it starts its
HTTP listener. Systemd starts the collaboration server after the local cloud
is ready. The `zed.target` target contains the credentials service, the local
cloud service, and the collaboration service. It does not contain PostgreSQL
or LiveKit.

## Authelia example

This example uses the current Authelia OIDC client schema. See the official
[Authelia OIDC client reference](https://www.authelia.com/configuration/identity-providers/openid-connect/clients/).

First, generate a client secret and its hash. Authelia prints the clear secret
and the PBKDF2 hash.

```sh
authelia crypto hash generate pbkdf2 \
  --variant sha512 \
  --random \
  --random.length 72 \
  --random.charset rfc3986
```

Store the clear secret in `/run/secrets/zed-oidc-client-secret`. Store the
hash in the Authelia configuration. Do not put the clear secret in the Nix
store.

Add this client to the Authelia configuration:

```yaml
identity_providers:
  oidc:
    # Keep the other required OIDC provider settings here.
    clients:
      - client_id: zed-local
        client_name: Zed Local Cloud
        client_secret: "<PBKDF2_HASH_FROM_THE_COMMAND>"
        public: false
        authorization_policy: two_factor
        require_pkce: true
        pkce_challenge_method: S256
        redirect_uris:
          - https://zed.example.com/auth/oidc/callback
        scopes:
          - openid
          - profile
          - email
          - groups
        response_types:
          - code
        grant_types:
          - authorization_code
        access_token_signed_response_alg: none
        userinfo_signed_response_alg: none
        token_endpoint_auth_method: client_secret_basic
```

Add each local administrator to the `zed-admin` group in Authelia. Use this
local cloud configuration:

```nix
services.zed-local-cloud.oidc = {
  issuerUrl = "https://auth.example.com";
  clientId = "zed-local";
  clientSecretFile = "/run/secrets/zed-oidc-client-secret";
  scopes = ["profile" "email" "groups"];
  adminClaim = "groups";
  adminValue = "zed-admin";
};
```

Authelia supplies the `groups` claim when the client requests the `groups`
scope. The service requests `openid` automatically. Do not add `openid` to
the Nix `scopes` list.

Authelia can supply the standard `picture` claim as part of profile data. If
Authelia has this value for a user, the local cloud uses it as the Zed avatar
URL.

For secret generation details, see the official
[Authelia OIDC FAQ](https://www.authelia.com/integration/openid-connect/frequently-asked-questions/).

## Standalone installation

Use a fresh database for the first installation. Apply the fixed collaboration
schema from this project:

```sh
createdb -U postgres zed
psql -U postgres -d zed \
  -f /path/to/zed-local-cloud/migrations/20251208000000_test_schema.sql
```

Do not apply the schema file a second time. The schema file is not idempotent.

Create a raw internal API key file. Replace the example paths with absolute
paths for your installation.

```sh
umask 077
openssl rand -out /var/lib/zed/cookie-key 64
openssl rand -base64 128 | tr -d '\n' > /var/lib/zed/internal-api-key
```

Store the OIDC client secret in a different file. The file must contain only
the raw client secret. Do not add a key name, quotes, or TOML syntax.

Create `zed-local-cloud.toml`:

```toml
base_url = "https://zed.example.com"
bind_address = "127.0.0.1:8787"
database_url = "postgres://postgres@localhost/zed"
log_level = "zed_local_cloud=info,sqlx=warn"

[oidc]
issuer_url = "https://auth.example.com"
client_id = "zed-local"
scopes = ["profile", "email", "groups"]
admin_claim = "groups"
admin_value = "zed-admin"
```

The local cloud constructs the OIDC redirect URL from `base_url`. It sets the
path to `/auth/oidc/callback` and keeps the configured scheme and authority.

Put files named `cookie-key`, `internal-api-key`, and `oidc-client-secret` in
one private directory. The cookie key must contain at least 64 bytes. Set
`CREDENTIALS_DIRECTORY` to that directory when you run the service outside
systemd.

Start the collaboration server from the compatible Zed source tree:

```sh
cd /path/to/zed
nix develop -c cargo run --package=collab -- serve collab
```

Start the local cloud:

```sh
cd /path/to/zed-local-cloud
CREDENTIALS_DIRECTORY=/run/secrets \
  nix develop -c cargo run -- --config ./zed-local-cloud.toml
```

The local cloud performs OIDC discovery during startup. Startup fails if the
provider is not available or its discovery data is not valid.

## Zed configuration

Set the public service URL before you start Zed:

```sh
cd /path/to/zed
ZED_SERVER_URL=https://zed.example.com \
  nix develop -c cargo run --package=zed
```

Select **Sign In** in Zed. The local cloud redirects the browser to the
configured OIDC provider. If the provider session is still valid, it redirects
directly back without another prompt. Each completed OIDC flow refreshes the
local profile and administrator status.

## Caddy routes

The NixOS module uses a small Caddy route table:

| Public path                       | Destination                                     |
| --------------------------------- | ----------------------------------------------- |
| `/rpc`                            | Redirect to `/actual_rpc` on the same authority |
| `/actual_rpc`                     | Local collaboration server `/rpc`               |
| `/extensions` and `/extensions/*` | `https://api.zed.dev`                           |
| `/releases` and `/releases/*`     | `https://cloud.zed.dev`                         |
| All other paths                   | Local cloud service                             |

The Rust service supplies `/models`. It returns an empty hosted-model catalog.
The service does not proxy Zed-hosted language models.

The service returns `204` for supported feedback routes. Other telemetry
routes do not have a handler and return `404`. No telemetry route goes to an
upstream server.

## Local cloud routes

The local cloud supplies these public client routes:

- `GET /native_app_signin`
- `GET /auth/oidc/callback`
- `GET /native_app_signin_succeeded`
- `GET /client/users/me`
- `GET /client/users/connect`
- `PATCH` or `POST /client/system_settings`
- `POST /client/llm_tokens`
- `POST /client/feedback/*`
- `GET /models`
- `GET /healthz`

The local cloud supplies these internal collaboration routes:

- `POST /internal/users/look_up_by_legacy_id`
- `POST /internal/users/look_up_by_github_login`
- `POST /internal/users/fuzzy_search`
- `POST /internal/channel_members/fuzzy_search`

The collaboration server uses the shared internal API key as a Bearer token.
Users appear in local search results only after their first local OIDC login.

## Configuration

Start `zed-local-cloud` with `--config` and the path to one TOML file. In
production, application settings come from this file and secrets come from
systemd credentials. `RUST_LOG` can override the configured tracing filter.

| TOML key           | Default                          | Function                                                  |
| ------------------ | -------------------------------- | --------------------------------------------------------- |
| `base_url`         | Required                         | Set the public local cloud URL used for the OIDC callback |
| `bind_address`     | `127.0.0.1:8787`                 | Set the local cloud TCP listen address                    |
| `database_url`     | Local `zed` socket URL           | Set the shared PostgreSQL URL                             |
| `log_level`        | `zed_local_cloud=info,sqlx=warn` | Set the tracing filter                                    |
| `oidc.issuer_url`  | Required                         | Set the OIDC issuer URL                                   |
| `oidc.client_id`   | Required                         | Set the OIDC client identifier                            |
| `oidc.scopes`      | `profile`, `email`, `groups`     | Set the additional OIDC scopes                            |
| `oidc.admin_claim` | `groups`                         | Set the administrator claim path                          |
| `oidc.admin_value` | `admin`                          | Set the administrator claim value                         |

Do not put secret values in the TOML file. The service requires the systemd
credentials `cookie-key`, `internal-api-key`, and `oidc-client-secret`. The
cookie key is binary and is read unchanged. Leading and trailing white space is
removed from the two text credentials.

## Database ownership

The collaboration server owns the `users` table and all collaboration foreign
keys. The local cloud adds these tables:

- `local_user_profiles` for local profile data
- `local_user_tokens` for local token hashes
- `local_oidc_identities` for issuer and subject links

An encrypted private cookie holds the OIDC flow state and RSA public key. A
successful OIDC callback consumes that cookie.

SQLx uses one migration ledger for the collaboration server and the local
cloud. Each migrator ignores migration versions that belong to the other
component.

The project keeps a fixed copy of the initial collaboration schema. Its SQLx
version is `20251208000000`. Its SQLx SHA-384 checksum is
`9c7061b50bfb256cba83454be75fbed99be601e9214e80ba0bf461d90c346bb2ae26ab0d7219aab086b3f9d51d18433d`.
This checksum is the checksum in databases that this project created before
the Zed 1.17.2 update.

The `zed-migrations` package contains the fixed schema and all incremental
migrations. The collab package does not refer to these files. A migration
change does not rebuild the collab package.

The upstream schema file can change without a new migration version. Do not
replace the fixed schema file when you update Zed. Compare the new upstream
snapshot with the fixed schema. If the schema changed, add a new incremental
migration to `migrations`. The migration must change an existing database to
the new schema.

## Security limits

The local cloud accepts only the OIDC authorization code flow. It requires
PKCE with SHA-256. It validates the callback state and nonce. It validates the
ID-token signature, issuer, audience, expiry, and access-token hash.

The private OIDC flow cookie is HttpOnly, SameSite=Lax, and limited to the root
path. Use HTTPS for the public service.

The extension proxy still uses a Zed-operated service. Zed service terms can
apply to this access. This project is unofficial. Zed does not document this
configuration as a supported self-hosted service.

## Development VM and web UI

Start the development backend VM first:

```sh
nix run
```

The VM runs PostgreSQL, the Zed collaboration server, Caddy, and
`oidc-provider-mock`. It does not run the local cloud executable. It forwards
PostgreSQL to `127.0.0.1:5432`, OIDC to `http://localhost:9091`, and the
unified Zed endpoint to `http://localhost:8787`. The tracked `.env` points
SQLx at this database.

The VM collaboration service applies all migrations in this project before it
starts. The host-side local cloud finds the same migration ledger and does not
apply the migrations a second time.

The Nix development shell includes the version-matched Topcoat CLI and the
Tailwind CSS CLI. In a second terminal, start the local cloud and UI:

```sh
nix develop -c topcoat dev
```

Open `http://localhost:8787/native_app_signin` with the query parameters sent
by Zed, or set `ZED_SERVER_URL=http://localhost:8787` in a development Zed
client. `topcoat dev` selects the built-in development configuration. The
application still uses the real database, OIDC, cookie, and route flows; only
the configuration source and asset-reload mode differ from production.

Topcoat rebuilds the Rust application and the Tailwind stylesheet when a
source file changes. The shared layout includes `topcoat::dev::script()`,
which reloads the browser after the new application is ready and renders
nothing outside `topcoat dev`.

## Tests

Run the Rust tests:

```sh
nix develop -c cargo test --locked
```

Run all flake checks:

```sh
nix flake check
```

Run the NixOS integration test:

```sh
nix build .#checks.x86_64-linux.nixos-basic
```

Start the NixOS integration test in interactive mode:

```sh
nix build '.#checks.x86_64-linux.nixos-basic.driverInteractive'
./result/bin/nixos-test-driver
```

The virtual machine uses `oidc-provider-mock`. The test checks OIDC discovery,
PKCE, token validation, UserInfo, and the complete native-app callback flow.

The virtual machine also uses a local extension API. The test checks the list
path and a nested download path. It checks query parameters. It also checks
that Caddy removes local authorization data and cookies.

## Known limits

`LISTEN`/`NOTIFY`, including across multiple local-cloud processes.

- Hosted Zed AI is disabled for the local organization.
- The service has no user-management page.
- The extension proxy depends on the public behavior of the Zed extension service.

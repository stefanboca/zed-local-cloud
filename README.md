# Zed local cloud

This project provides a local cloud service for an unmodified Zed client. It
also provides a local Zed collaboration server.

The local cloud uses OpenID Connect (OIDC) for user sign-in. It does not use a
Zed account. It does not send login data to Zed Cloud.

This design does not require a login to a Zed server. The current Zed baseline
does not require a login for extension access. Caddy sends extension requests
to `https://api.zed.dev`. It sends release requests to
`https://cloud.zed.dev`. All other cloud and web requests stay on the local
system. Caddy removes local authorization data and cookies from extension
requests.

The upstream extension service can change. Test extension installation after
each Zed update.

This README uses ASD-STE100 Simplified Technical English (STE).

## Compatibility baseline

The project uses
[Zed v1.20.1](https://github.com/zed-industries/zed/tree/v1.20.1) as its
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
framework. It uses encrypted private cookies for the OIDC transaction. The
[openidconnect](https://docs.rs/openidconnect/4.0.1/openidconnect/) crate does
OIDC discovery, PKCE, token exchange, and token verification.

The local cloud and the collaboration server use the same PostgreSQL database.
They also use the same internal API key.

## Request flow

These steps describe the sign-in flow:

1. Zed opens `/native_app_signin` in the browser.
2. The local cloud validates the Zed RSA public key.
3. The local cloud stores the OIDC state and typed RSA key in a private cookie.
4. The local cloud redirects the browser to the OIDC code flow with PKCE.
5. The OIDC provider authenticates the user.
6. The local cloud verifies the OIDC state, nonce, ID token, and access-token hash.
7. The local cloud reads profile data from the ID token.
8. The local cloud reads more profile data from the UserInfo endpoint.
9. The local cloud writes the user and administrator status to PostgreSQL.
10. The local cloud creates and encrypts a new local Zed access token.
11. The browser sends the encrypted token to the local Zed callback port.

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

The administrator claim defaults to `groups`. The default scopes include the
`groups` scope. A string value or an array value is valid. Dot notation selects
a nested claim. For example,
`realm_access.roles` selects the `roles` member of `realm_access`.

The local cloud checks the administrator claim after each successful login.
It sets `users.admin` to `true` when the configured value is present. It sets
`users.admin` to `false` when the value is not present. The OIDC provider is
the source of the local administrator role.

## NixOS installation

The flake exports these items:

- The `zed-local-cloud` package for the local cloud
- The `zed-collab` package for the collaboration server
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
            livekit.apiKey = "zed";

            localCloud.settings.oidc = {
              issuer_url = "https://auth.example.com";
              client_id = "zed-local";
              scopes = ["profile" "email" "groups"];
              admin_claim = "groups";
              admin_value = "zed-admin";
            };
            secrets.oidcClientSecretFile = "/run/secrets/zed-oidc-client-secret";
          };

          services.livekit.keyFile = "/run/secrets/livekit-keys";

          services.caddy.openFirewall = true;
        }
      ];
    };
  };
}
```

The module uses `virtualHost` for the default local cloud `base_url`. The
default URL uses the `https` scheme. The local cloud adds
`/auth/oidc/callback` to this URL. Register this callback URL with the OIDC
provider.

The module writes `localCloud.settings` directly to the local cloud TOML file.
It passes `collab.environment` directly to the collab process. The module
declares and checks all settings that it uses. You can add more TOML settings to
`localCloud.settings`. The local cloud rejects settings that its configuration
parser does not support. You can add scalar environment variables to
`collab.environment`. A scalar value can be a string, integer, float, or Boolean
value.

The upstream NixOS `services.livekit` module enables LiveKit by default. Caddy
publishes LiveKit at `https://<virtualHost>/livekit`. The collab server receives
this public URL and the selected API key pair. The integration opens only the
configured UDP RTC port range. Caddy keeps the TCP API port behind its proxy.
Configure ports, RTC IP behavior, Redis, and the package under
`services.livekit`. Set
`services.zed-local-cloud.livekit.enable = false` to disable the whole
integration.

The administrator controls `services.livekit.keyFile`. The file can contain
keys for multiple applications. It uses the LiveKit `key: secret` YAML format.
Set `services.zed-local-cloud.livekit.apiKey` to the exact key name for Zed.
Systemd supplies the complete file as the `LIVEKIT_KEYS` credential. SecretSpec
reads this credential. The provision command selects the specified entry. It
writes the selected key and secret to separate files in the Zed state directory.
It does not copy the complete key file to the Nix store.

Systemd loads `secrets.oidcClientSecretFile` as the `OIDC_CLIENT_SECRET`
credential. The local cloud reads the raw secret from its private credential
directory. The TOML configuration does not contain the secret or its path. The
service uses a confidential OIDC client and requires this secret.

The module creates a local PostgreSQL database by default. It creates the
`zed` system user and the `zed` database. It uses PostgreSQL peer
authentication.

Set `services.zed-local-cloud.database.createLocally = false` for an external
database. Then set `services.zed-local-cloud.secrets.databaseUrlFile` to a
runtime secret file. This file must contain the external database URL.

The `zed-provision` service uses an explicit file provider to create two keys on
its first run. It creates a 64-byte cookie key and a 128-byte internal API key.
It stores their base64 text in `/var/lib/zed/COOKIE_KEY` and
`/var/lib/zed/INTERNAL_API_KEY`. Both files have mode `0600`. Later runs use the
same values. During each activation, the service also runs the embedded SQLx
migrations and updates the `pg_trgm` extension. If LiveKit is enabled, it writes
`LIVEKIT_KEY` and `LIVEKIT_SECRET` to the same state directory.

Systemd mounts the required files as service credentials. The local cloud reads
its credentials through the SecretSpec Rust API. The collab service runs under
`secretspec run`. SecretSpec puts only the `collab` scope in the collab process
environment. The module does not create collab `.env.toml` files.

The provision service does not depend on the local cloud service. Thus, it can
prepare secrets and migrations when `localCloud.enable` is `false`.

The administrator supplies the OIDC client secret and the LiveKit key file.
Systemd passes them as credentials. SecretSpec reads the credentials, but it
does not create them.

The provision service applies all migrations before either application starts.
Systemd starts the collaboration server after the local cloud is ready. The
`zed.target` target contains the provision, local cloud, and collaboration
services. It does not contain PostgreSQL or LiveKit.

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
services.zed-local-cloud.localCloud.settings.oidc = {
  issuer_url = "https://auth.example.com";
  client_id = "zed-local";
  scopes = ["profile" "email" "groups"];
  admin_claim = "groups";
  admin_value = "zed-admin";
};
services.zed-local-cloud.secrets.oidcClientSecretFile =
  "/run/secrets/zed-oidc-client-secret";
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

Use a fresh PostgreSQL database for the first installation:

```sh
createdb -U postgres zed
```

Create a private directory for the SecretSpec file provider. Put the database
URL in a file named `DATABASE_URL`. Put the OIDC secret in a file named
`OIDC_CLIENT_SECRET`. Each file must contain only its secret value.

```sh
umask 077
mkdir -p /run/secrets/zed
printf '%s' 'postgres:///zed' > /run/secrets/zed/DATABASE_URL
printf '%s' 'replace-this-value' > /run/secrets/zed/OIDC_CLIENT_SECRET
```

Replace the example values before you continue. Do not add a key name, quotes,
or TOML syntax to a secret file.

Create `zed-local-cloud.toml`:

```toml
base_url = "https://zed.example.com"
bind_address = "127.0.0.1:8787"
log_level = "zed_local_cloud=info,sqlx=warn"

[oidc]
issuer_url = "https://auth.example.com"
client_id = "zed-local"
scopes = ["profile", "email", "groups"]
admin_claim = "groups"
admin_value = "zed-admin"
```

The local cloud makes the OIDC redirect URL from `base_url`. It adds the
`/auth/oidc/callback` path. It keeps the configured scheme and authority.

Run the provision command before you start either application:

```sh
cd /path/to/zed-local-cloud
SECRETSPEC_PROVIDER=file:/run/secrets/zed \
  nix develop -c cargo run -- provision \
  --state-directory /run/secrets/zed
```

The provision command creates `COOKIE_KEY` and `INTERNAL_API_KEY` in the state
directory. It applies all database migrations and updates `pg_trgm`.

For LiveKit, add a `LIVEKIT_KEYS` file to the secret directory. Use LiveKit
YAML syntax in this file. Set `LIVEKIT_SERVER` in the collab environment.
Pass the selected key name with `--livekit-api-key` to the provision command.

Start the collaboration server from the compatible Zed source tree:

```sh
cd /path/to/zed
SECRETSPEC_PROVIDER=file:/run/secrets/zed \
  secretspec --file /path/to/zed-local-cloud/secretspec.toml \
  run --profile production --scope collab -- \
  nix develop -c cargo run --package=collab -- serve collab
```

Start the local cloud:

```sh
cd /path/to/zed-local-cloud
SECRETSPEC_PROVIDER=file:/run/secrets/zed \
  nix develop -c cargo run -- serve --config ./zed-local-cloud.toml
```

The local cloud does OIDC discovery during startup. Startup fails if the
provider is not available. Startup also fails if the discovery data is not
valid.

## Zed configuration

Open the Zed settings file. Set `server_url` to the public service URL:

```json
{
  "server_url": "https://zed.example.com"
}
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
The `/client/users/connect` route uses PostgreSQL `LISTEN` and `NOTIFY`. Each
client receives updates only for its signed-in user. This works across multiple
local cloud processes. The route sends the cloud WebSocket `UserUpdated`
message after a local profile or administrator status changes. The baseline
also defines a `NotificationsUpdated` message, but its client does not act on
that message. This service does not send it. Add it only if a later Zed client
uses it and the local cloud supplies the related notification state and API.

## Configuration

Start `zed-local-cloud serve` with `--config` and the path to one TOML file.
This file contains the application settings. Systemd credentials contain the
secrets that the administrator supplies. The provision command stores generated
secrets in its explicit state directory. `RUST_LOG` can override the configured
tracing filter.

| TOML key           | Default                                              | Function                                                  |
| ------------------ | ---------------------------------------------------- | --------------------------------------------------------- |
| `base_url`         | Required                                             | Set the public local cloud URL used for the OIDC callback |
| `bind_address`     | `127.0.0.1:8787`                                     | Set the local cloud TCP listen address                    |
| `log_level`        | `zed_local_cloud=info,tower_http=info,sqlx=warn`      | Set the tracing filter                                    |
| `oidc.issuer_url`  | Required                                             | Set the OIDC issuer URL                                   |
| `oidc.client_id`   | Required                                             | Set the OIDC client identifier                            |
| `oidc.scopes`      | `profile`, `email`, `groups`                         | Set the additional OIDC scopes                            |
| `oidc.admin_claim` | `groups`                                             | Set the administrator claim path                          |
| `oidc.admin_value` | `admin`                                              | Set the administrator claim value                         |

Do not put secret values in the configuration TOML file. In production,
systemd mounts `COOKIE_KEY` and `INTERNAL_API_KEY` from `/var/lib/zed`. It also
mounts `DATABASE_URL` and `OIDC_CLIENT_SECRET` from the configured source files.
SecretSpec reads these values from the systemd credential directory. SecretSpec
stores the cookie key as base64 text. The application removes surrounding white
space from the database URL and the application keys. It also removes this
white space from the OIDC secret.

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

The local-cloud executable embeds the fixed schema and all incremental
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

The VM provisioning service applies all migrations in this project before
collab starts. The host-side local cloud finds the same migration ledger and
does not apply the migrations a second time.

A changed query needs new SQLx offline metadata. The normal VM build cannot use
a query before this metadata exists. Run the preparation app from the repository
root:

```sh
nix run .#sqlx-prepare
```

The app starts a temporary PostgreSQL instance. It applies all migrations and
updates `pg_trgm`. It then runs `cargo sqlx prepare` and stops PostgreSQL. The
app removes the temporary database when it exits. It does not build or run the
local cloud application before PostgreSQL is ready.

The Nix development shell includes the version-matched Topcoat CLI and the
Tailwind CSS CLI. In a second terminal, start the local cloud and UI:

```sh
nix develop -c topcoat dev
```

Topcoat invokes the binary without arguments, which defaults to `serve` in the
detected development configuration. Production invocations use
`zed-local-cloud serve --config <path>` explicitly.

Set `server_url` to `http://localhost:8787` in the development Zed settings.
You can also open `http://localhost:8787/native_app_signin` with the required
query parameters. `topcoat dev` selects the built-in development configuration.
The application still uses the real database, OIDC, cookie, and route flows.
Only the configuration source and asset-reload mode differ from production.

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

- Hosted Zed AI is disabled for the local organization.
- The service has no user-management page.
- The extension proxy depends on the public behavior of the Zed extension service.

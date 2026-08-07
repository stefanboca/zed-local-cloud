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
[Zed v1.14.2](https://github.com/zed-industries/zed/tree/v1.14.2) as its
compatibility baseline. This release has commit
`02abf5b08fa12c1c20a155ae3f796ef4c6c1a01e`.

The baseline defines these interfaces:

- The native application sign-in query and callback
- The `/client/users/me` response
- The collaboration internal user API
- The collaboration WebSocket headers
- The collaboration database schema

Check these interfaces before you update Zed.

## Components

The default installation has these components:

| Component | Default address | Function |
| --- | --- | --- |
| PostgreSQL | Local socket | Store collaboration and local cloud data |
| Zed collaboration server | `127.0.0.1:8080` | Provide collaboration RPC |
| Zed local cloud | `127.0.0.1:8787` | Provide login and cloud API routes |
| Caddy | Public HTTPS address | Route public requests |
| OIDC provider | External or local address | Authenticate users |

The local cloud uses the [Topcoat](https://github.com/tokio-rs/topcoat) web
framework. It uses Topcoat sessions for the browser login transaction. It uses
the [openidconnect](https://docs.rs/openidconnect/4.0.1/openidconnect/) crate
for OIDC discovery, PKCE, token exchange, and token verification.

The local cloud and the collaboration server use the same PostgreSQL database.
They also use the same internal API key.

## Request flow

The sign-in flow is as follows:

1. Zed opens `/native_app_signin` in the browser.
2. The local cloud validates the Zed RSA public key.
3. The local cloud starts a Topcoat browser session.
4. The user selects the OIDC sign-in button.
5. The local cloud starts the OIDC authorization code flow with PKCE.
6. The OIDC provider authenticates the user.
7. The local cloud verifies the OIDC state, nonce, ID token, and access-token hash.
8. The local cloud reads profile data from the ID token and the UserInfo endpoint.
9. The local cloud writes the user and administrator status to PostgreSQL.
10. The local cloud creates a new local Zed access token.
11. The local cloud encrypts the token with the Zed RSA public key.
12. The browser sends the encrypted token to the local Zed callback port.

The service stores only a SHA-256 hash of the local Zed access token. It does
not store the OIDC access token or the OIDC client secret in PostgreSQL.

## OIDC claims

The local cloud uses the OIDC issuer and subject as the permanent user
identity. It reads these standard claims:

| Claim | Use |
| --- | --- |
| `preferred_username` | Create the local username |
| `email` | Supply an email address and a username fallback |
| `name` | Supply the display name |
| `picture` | Supply the Zed `avatar_url` |

The service reads claims from the verified ID token. If the provider has a
UserInfo endpoint, the service also reads claims from that endpoint. A
UserInfo value has priority over the same ID-token value.

The `picture` claim is optional. If it is not present, `avatar_url` is an
empty string.

The administrator claim is configurable. A string value or an array value is
valid. Dot notation selects a nested claim. For example,
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

            oidc = {
              issuerUrl = "https://auth.example.com";
              clientId = "zed-local";
              clientSecretFile = "/run/secrets/zed-oidc-client-secret";
              redirectUrl = "https://zed.example.com/auth/oidc/callback";
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

Systemd loads `clientSecretFile` as a credential. The local cloud reads the
raw secret from its private credential directory. The TOML configuration
contains only the credential path.

The module creates a local PostgreSQL database by default. It creates the
`zed` system user and the `zed` database. It uses PostgreSQL peer
authentication.

Set `services.zed-local-cloud.database.createLocally = false` for an external
database. Then set `services.zed-local-cloud.database.url` to the external
database URL.

The `zed-setup` service creates a 128-byte internal API key on its first run.
It writes only the raw key to `/var/lib/zed/internal-api-key`. Later runs keep
the same non-empty file. The file mode is `0600`. The generated TOML
configuration contains the path to this file. It does not contain the key.

Each secret has a separate file. Each secret file contains only the raw
secret. Do not use TOML syntax in a secret file.

The setup service applies the collaboration schema. The local cloud applies
its embedded migrations before it starts its HTTP listener.

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
        client_secret: '<PBKDF2_HASH_FROM_THE_COMMAND>'
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
  redirectUrl = "https://zed.example.com/auth/oidc/callback";
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

Use a fresh database for the first installation. Apply the Zed collaboration
schema from the compatible Zed source tree:

```sh
createdb -U postgres zed
psql -U postgres -d zed \
  -f /path/to/zed/crates/collab/migrations/20251208000000_test_schema.sql
```

Do not apply the schema file a second time. The schema file is not idempotent.

Create a raw internal API key file. Replace the example paths with absolute
paths for your installation.

```sh
umask 077
openssl rand -base64 128 | tr -d '\n' > /var/lib/zed/internal-api-key
```

Store the OIDC client secret in a different file. The file must contain only
the raw client secret. Do not add a key name, quotes, or TOML syntax.

Create `zed-local-cloud.toml`:

```toml
bind_address = "127.0.0.1:8787"
database_url = "postgres://postgres@localhost/zed"
internal_api_key_file = "/var/lib/zed/internal-api-key"
log_level = "zed_local_cloud=info,sqlx=warn"

[oidc]
issuer_url = "https://auth.example.com"
client_id = "zed-local"
client_secret_file = "/run/secrets/zed-oidc-client-secret"
redirect_url = "https://zed.example.com/auth/oidc/callback"
scopes = ["profile", "email", "groups"]
admin_claim = "groups"
admin_value = "zed-admin"
```

Start the collaboration server from the compatible Zed source tree:

```sh
cd /path/to/zed
nix develop -c cargo run --package=collab -- serve collab
```

Start the local cloud:

```sh
cd /path/to/zed-local-cloud
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

Select **Sign In** in Zed. The browser shows the local Topcoat login page. The
browser then opens the configured OIDC provider.

## Caddy routes

The NixOS module uses a small Caddy route table:

| Public path | Destination |
| --- | --- |
| `/rpc` | Redirect to `/actual_rpc` on the same authority |
| `/actual_rpc` | Local collaboration server `/rpc` |
| `/extensions` and `/extensions/*` | `https://api.zed.dev` |
| All other paths | Local cloud service |

The Rust service supplies `/models`. It returns an empty hosted-model catalog.
The service does not proxy Zed-hosted language models.

The service returns `204` for supported feedback routes. Other telemetry
routes do not have a handler and return `404`. No telemetry route goes to an
upstream server.

## Local cloud routes

The local cloud supplies these public client routes:

- `GET /native_app_signin`
- `POST /auth/oidc/start`
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

Start `zed-local-cloud` with `--config` and the path to one TOML file. The
service does not read its configuration from environment variables.

| TOML key | Default | Function |
| --- | --- | --- |
| `bind_address` | `127.0.0.1:8787` | Set the local cloud listen address |
| `database_url` | Local `zed` socket URL | Set the shared PostgreSQL URL |
| `internal_api_key_file` | Required | Set the path to the raw internal API key |
| `log_level` | `zed_local_cloud=info,sqlx=warn` | Set the tracing filter |
| `oidc.issuer_url` | Required | Set the OIDC issuer URL |
| `oidc.client_id` | Required | Set the OIDC client identifier |
| `oidc.client_secret_file` | Empty | Set the path to the raw OIDC client secret |
| `oidc.redirect_url` | Required | Set the registered callback URL |
| `oidc.scopes` | `profile`, `email` | Set the additional OIDC scopes |
| `oidc.admin_claim` | `roles` | Set the administrator claim path |
| `oidc.admin_value` | `admin` | Set the administrator claim value |

Do not put secret values in the TOML file. Use one raw file for each secret.
The service removes leading and trailing white space when it reads a secret.

## Database ownership

The collaboration server owns the `users` table and all collaboration foreign
keys. The local cloud adds these tables:

- `local_user_profiles` for local profile data
- `local_user_tokens` for local token hashes
- `local_oidc_identities` for issuer and subject links
- `local_login_sessions` for short browser login transactions

The login-session table stores the Topcoat session token hash. It does not
store the browser session token.

SQLx uses one migration ledger for the collaboration server and the local
cloud. Each migrator ignores migration versions that belong to the other
component.

The upstream collaboration migration is a schema snapshot. Its version can
stay the same when upstream changes its contents. SQLx rejects an old database
if the recorded checksum is different. Add a local incremental migration
before you update to a Zed release with a new schema.

## Security limits

The local cloud accepts only the OIDC authorization code flow. It requires
PKCE with SHA-256. It validates the callback state and nonce. It validates the
ID-token signature, issuer, audience, expiry, and access-token hash.

The default Topcoat session cookie is Secure, HttpOnly, SameSite=Lax, and
limited to the root path. Use HTTPS for the public service.

The extension proxy still uses a Zed-operated service. Zed service terms can
apply to this access. This project is unofficial. Zed does not document this
configuration as a supported self-hosted service.

## Web UI development

The Nix development shell includes the version-matched Topcoat CLI. Start the
UI development server:

```sh
nix develop -c topcoat dev --bin dev
```

Open `http://127.0.0.1:3000`. The index links to each UI preview. The mock
sign-in button opens the sign-in-complete preview. This mode does not read a
configuration file, connect to PostgreSQL, or perform OIDC discovery.

Topcoat rebuilds the Rust application when a source file changes. The `dev`
binary includes `topcoat::dev::script()`, which reloads the browser after the
new application is ready. Production routes render the shared pages with this
development behavior disabled.

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

The virtual machine uses a local OIDC test provider. The test checks OIDC
discovery, PKCE, token validation, UserInfo, the picture claim, and the
administrator role. The test grants the role and then removes it.

The virtual machine also uses a local extension API. The test checks the list
path and a nested download path. It checks query parameters. It also checks
that Caddy removes local authorization data and cookies.

## Known limits

- `/client/users/connect` keeps the WebSocket open. It does not send account-change events.
- Hosted Zed AI is disabled for the local organization.
- The service has no user-management page.
- The extension proxy depends on the public behavior of the Zed extension service.
- Perform a two-user collaboration acceptance test before production use.

{
  self,
  pkgs,
}: let
  inherit
    (self.packages.${pkgs.stdenv.hostPlatform.system})
    oidc-provider-mock
    zed-collab
    zed-local-cloud
    zed-migrations
    ;
in {
  nixos-basic = pkgs.testers.runNixOSTest {
    name = "zed-local-cloud-basic";

    nodes.machine = {
      imports = [
        self.nixosModules.default
        (import ../../dev/oidc-provider-mock.nix {
          inherit oidc-provider-mock;
          port = 18080;
        })
      ];

      services.zed-local-cloud = {
        enable = true;
        package = zed-local-cloud;
        migrations.package = zed-migrations;
        collab.package = zed-collab;
        virtualHost = "localhost";
        upstreamApiUrl = "http://127.0.0.1:18081";
        upstreamCloudUrl = "http://127.0.0.1:18081";
        oidc = {
          adminClaim = "groups";
          adminValue = "zed-admin";
          issuerUrl = "http://127.0.0.1:18080";
          clientId = "zed-local";
          clientSecretFile = toString (pkgs.writeText "test-oidc-client-secret" "test-client-secret");
        };
      };

      systemd.services.fake-extensions = {
        wantedBy = ["multi-user.target"];
        before = ["caddy.service"];
        serviceConfig.ExecStart = "${pkgs.python3}/bin/python ${../fake-extensions.py}";
      };
      systemd.services.zed-local-cloud = {
        requires = ["oidc-provider-mock.service"];
        after = ["oidc-provider-mock.service"];
      };
    };

    testScript = ''
      import base64
      import hashlib
      import html
      import json
      import re
      import shlex
      import tomllib
      from urllib.parse import parse_qs, quote, urlsplit

      start_all()

      for unit in [
          "postgresql.target",
          "fake-extensions.service",
          "oidc-provider-mock.service",
          "zed-credentials.service",
          "zed-local-cloud.service",
          "zed-collab.service",
          "zed.target",
          "livekit.service",
          "caddy.service",
      ]:
          machine.wait_for_unit(unit)

      machine.wait_for_open_port(443)
      machine.wait_for_open_port(8787)
      machine.wait_for_open_port(7880)

      failed_units = machine.succeed(
          "systemctl list-units --state=failed --no-legend --plain"
      )
      assert failed_units.strip() == "", failed_units

      for unit in [
          "zed-credentials.service",
          "zed-local-cloud.service",
          "zed-collab.service",
      ]:
          user = machine.succeed(
              f"systemctl show {unit} --property=User --value"
          ).strip()
          assert user == "zed", f"{unit} runs as {user!r}"

          state_directory = machine.succeed(
              f"systemctl show {unit} --property=StateDirectory --value"
          ).strip()
          assert state_directory == "zed", (
              f"{unit} uses state directory {state_directory!r}"
          )

      zed_target_wants = set(machine.succeed(
          "systemctl show zed.target --property=Wants --value"
      ).split())
      assert zed_target_wants == {
          "zed-credentials.service",
          "zed-local-cloud.service",
          "zed-collab.service",
      }, zed_target_wants
      assert machine.succeed(
          "systemctl show zed-local-cloud.service --property=Type --value"
      ).strip() == "notify"
      collab_after = set(machine.succeed(
          "systemctl show zed-collab.service --property=After --value"
      ).split())
      assert "zed-local-cloud.service" in collab_after, collab_after

      assert machine.succeed(
          "stat --format='%U:%G %a' /var/lib/zed"
      ).strip() == "zed:zed 700"
      assert machine.succeed(
          "stat --format='%U:%G %a' /var/lib/zed/internal-api-key"
      ).strip() == "zed:zed 600"
      assert machine.succeed(
          "stat --format='%U:%G %a' /var/lib/zed/livekit-keys"
      ).strip() == "zed:zed 600"
      for livekit_credential in ["livekit-api-key", "livekit-api-secret"]:
          assert machine.succeed(
              "stat --format='%U:%G %a' "
              f"/var/lib/zed/{livekit_credential}"
          ).strip() == "zed:zed 600"
      key = machine.succeed("cat /var/lib/zed/internal-api-key")
      assert len(base64.b64decode(key, validate=True)) == 128
      livekit_key = machine.succeed("cat /var/lib/zed/livekit-api-key")
      livekit_secret = machine.succeed("cat /var/lib/zed/livekit-api-secret")
      assert len(livekit_key) >= 12
      assert len(livekit_secret) >= 32
      assert machine.succeed(
          "cat /var/lib/zed/livekit-keys"
      ) == f"{livekit_key}: {livekit_secret}\n"

      auth_environment = machine.succeed(
          "systemctl show zed-local-cloud.service --property=Environment --value"
      )
      for name in [
          "BIND_ADDRESS=",
          "DATABASE_URL=",
          "OIDC_",
          "RUST_LOG=",
          "ZED_CLOUD_INTERNAL_API_KEY=",
      ]:
          assert name not in auth_environment, auth_environment

      auth_pid = machine.succeed(
          "systemctl show zed-local-cloud.service --property=MainPID --value"
      ).strip()
      auth_command = machine.succeed(
          f"tr '\\0' '\\n' </proc/{auth_pid}/cmdline"
      ).splitlines()
      assert auth_command[-2] == "--config", auth_command
      auth_config_text = machine.succeed(f"cat {auth_command[-1]}")
      auth_config = tomllib.loads(auth_config_text)
      assert auth_config["base_url"] == "https://localhost"
      assert auth_config["bind_address"] == "127.0.0.1:8787"
      assert auth_config["database_url"] == (
          "postgres://zed@%2Frun%2Fpostgresql/zed"
      )
      assert "internal_api_key_file" not in auth_config
      assert "client_secret_file" not in auth_config["oidc"]
      assert "redirect_url" not in auth_config["oidc"]
      assert "test-client-secret" not in auth_config_text
      assert machine.succeed(
          "cat /run/credentials/zed-local-cloud.service/oidc-client-secret"
      ) == "test-client-secret"
      assert machine.succeed(
          "cat /run/credentials/zed-local-cloud.service/internal-api-key"
      ) == key

      collab_environment = tomllib.loads(
          machine.succeed("cat /run/zed-collab/crates/collab/.env.toml")
      )
      assert (
          collab_environment["DATABASE_URL"]
          == "postgres://zed@%2Frun%2Fpostgresql/zed"
      )
      assert collab_environment["ZED_CLOUD_URL"] == "http://127.0.0.1:8787"
      assert collab_environment["ZED_ENVIRONMENT"] == "production"
      assert collab_environment["LIVEKIT_SERVER"] == "https://localhost/livekit"
      collab_secret = tomllib.loads(
          machine.succeed(
              "cat /run/zed-collab/crates/collab/.env.secret.toml"
          )
      )
      assert collab_secret == {
          "ZED_CLOUD_INTERNAL_API_KEY": key,
          "LIVEKIT_KEY": livekit_key,
          "LIVEKIT_SECRET": livekit_secret,
      }
      assert machine.succeed(
          "cat /run/credentials/livekit.service/livekit-secrets"
      ) == f"{livekit_key}: {livekit_secret}\n"
      assert machine.succeed(
          "cat /run/credentials/zed-collab.service/livekit-api-key"
      ) == livekit_key
      assert machine.succeed(
          "cat /run/credentials/zed-collab.service/livekit-api-secret"
      ) == livekit_secret

      machine.succeed("systemctl restart zed-credentials.service")
      assert machine.succeed(
          "cat /var/lib/zed/internal-api-key"
      ) == key
      assert machine.succeed("cat /var/lib/zed/livekit-api-key") == livekit_key
      assert machine.succeed(
          "cat /var/lib/zed/livekit-api-secret"
      ) == livekit_secret

      migrations = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed --tuples-only --no-align "
          "--command='SELECT version FROM _sqlx_migrations ORDER BY version'"
      ).splitlines()
      assert migrations == [
          "20251208000000",
          "20260724000000",
          "20260806000000",
      ], migrations
      collab_schema_checksum = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed --tuples-only --no-align "
          "--command=\"SELECT encode(checksum, 'hex') FROM _sqlx_migrations "
          "WHERE version = 20251208000000\""
      ).strip()
      assert collab_schema_checksum == (
          "9c7061b50bfb256cba83454be75fbed99be601e9214e80ba0bf461d90c346bb2"
          "ae26ab0d7219aab086b3f9d51d18433d"
      ), collab_schema_checksum

      environments = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed --tuples-only --no-align "
          "--command='SELECT DISTINCT environment FROM servers'"
      ).splitlines()
      assert environments == ["production"], environments

      machine.succeed(
          "runuser -u zed -- psql --dbname=zed --set=ON_ERROR_STOP=1 "
          "--command='BEGIN; "
          "INSERT INTO users (id, admin, connected_once) "
          "VALUES (42, false, false); "
          "ROLLBACK;'"
      )

      def response(path):
          return machine.succeed(
              "curl --insecure --silent --show-error --output /dev/null "
              f"--write-out='%{{http_code}} %{{redirect_url}}' "
              f"https://localhost{path}"
          ).strip()

      def location(path):
          return machine.succeed(
              "curl --insecure --silent --show-error --output /dev/null "
              f"--write-out='%header{{location}}' https://localhost{path}"
          ).strip()

      assert response("/healthz") == "204"
      assert response("/livekit/").startswith("200"), response("/livekit/")
      assert machine.succeed(
          "curl --silent --show-error --output /dev/null "
          "--write-out='%{http_code}' http://127.0.0.1:8787/healthz"
      ).strip() == "204"
      assert response("/rpc") == "307 https://localhost/actual_rpc"
      assert location("/rpc") == "https://localhost/actual_rpc"
      rpc_location_with_port = machine.succeed(
          "curl --insecure --silent --show-error --output /dev/null "
          "--header='Host: localhost:18787' "
          "--write-out='%header{location}' https://localhost/rpc"
      ).strip()
      assert rpc_location_with_port == "https://localhost:18787/actual_rpc"
      assert response("/actual_rpc") == "401"
      extension_list = json.loads(machine.succeed(
          "curl --insecure --silent --show-error "
          "--header='Authorization: Bearer local-only' "
          "--header='Cookie: local-session=secret' "
          "'https://localhost/extensions?filter=rust'"
      ))
      assert extension_list == {
          "authorization": None,
          "cookie": None,
          "host": "127.0.0.1:18081",
          "path": "/extensions",
          "query": {"filter": ["rust"]},
          "source": "fake-extension-upstream",
      }, extension_list
      extension_download = json.loads(machine.succeed(
          "curl --insecure --silent --show-error "
          "'https://localhost/extensions/test.extension/download?version=1'"
      ))
      assert extension_download["path"] == (
          "/extensions/test.extension/download"
      ), extension_download
      assert extension_download["query"] == {
          "version": ["1"]
      }, extension_download
      assert extension_download["source"] == (
          "fake-extension-upstream"
      ), extension_download
      release_asset = json.loads(machine.succeed(
          "curl --insecure --silent --show-error "
          "'https://localhost/releases/stable/1.16.1/asset?asset=zed-remote-server'"
      ))
      assert release_asset["path"] == (
          "/releases/stable/1.16.1/asset"
      ), release_asset
      assert release_asset["query"] == {
          "asset": ["zed-remote-server"]
      }, release_asset
      assert release_asset["source"] == (
          "fake-extension-upstream"
      ), release_asset
      assert response("/client/users/me") == "401"
      assert machine.succeed(
          "curl --insecure --silent --show-error --output /dev/null "
          "--request POST "
          "--header='Content-Type: application/json' "
          "--data='{\"channel_id\":1,\"query\":\"local\",\"limit\":10}' "
          "--write-out='%{http_code}' "
          "https://localhost/internal/channel_members/fuzzy_search"
      ).strip() == "401"
      assert response("/models") == "200"
      models = machine.succeed(
          "curl --insecure --silent --show-error https://localhost/models"
      )
      assert json.loads(models) == {
          "models": [],
          "default_model": None,
          "default_fast_model": None,
          "recommended_models": [],
      }
      assert response("/native_app_signin_succeeded") == "200"
      assert response("/native_app_signin") == "400"
      assert response("/favicon.ico") == "200"
      favicon = machine.succeed(
          "curl --insecure --silent --show-error --include "
          "https://localhost/favicon.ico"
      )
      assert "content-type: image/svg+xml" in favicon.lower(), favicon
      assert "<svg" in favicon, favicon
      assert response("/channel/test-41767") == "200"
      channel_page = machine.succeed(
          "curl --insecure --silent --show-error "
          "https://localhost/channel/test-41767"
      )
      assert "zed://channel/test-41767" in channel_page, channel_page
      assert "Launching Zed" in channel_page, channel_page
      assert re.search(
          r"window\.location\.href = __external\d+\.toString\(\) "
          r"\+ window\.location\.hash",
          html.unescape(channel_page),
      ), channel_page
      assert response("/channel/test-41767/notes") == "200"
      notes_page = machine.succeed(
          "curl --insecure --silent --show-error "
          "https://localhost/channel/test-41767/notes"
      )
      assert "zed://channel/test-41767/notes" in notes_page, notes_page
      assert response("/channel/not-a-channel") == "404"
      assert response("/missing-page") == "404"
      missing_page = machine.succeed(
          "curl --insecure --silent --show-error https://localhost/missing-page"
      )
      assert "404 — Not Found" in missing_page, missing_page
      assert "The page could not be found" in missing_page, missing_page

      machine.succeed(
          "${pkgs.openssl}/bin/openssl genrsa "
          "-out /tmp/native-app-key.pem 2048"
      )
      native_public_key = machine.succeed(
          "${pkgs.openssl}/bin/openssl rsa "
          "-in /tmp/native-app-key.pem "
          "-RSAPublicKey_out -outform DER 2>/dev/null "
          "| base64 -w0 | tr '+/' '-_'"
      ).strip()

      def oidc_login():
          signin_url = (
              "https://localhost/native_app_signin"
              "?native_app_port=49152"
              f"&native_app_public_key={quote(native_public_key)}"
          )
          login_page = machine.succeed(
              "curl --insecure --silent --show-error "
              "--cookie /tmp/login-cookies "
              "--cookie-jar /tmp/login-cookies "
              f"{shlex.quote(signin_url)}"
          )
          assert "Authenticating with OpenID Connect" in login_page
          assert "You will automatically be redirected" in login_page
          redirect = re.search(r'cx\.hydrate\((.*?)\)', login_page)
          assert redirect is not None, login_page
          authorization_url = json.loads(html.unescape(redirect.group(1)))
          assert authorization_url.startswith(
              "http://127.0.0.1:18080/oauth2/authorize?"
          ), authorization_url

          callback_url = machine.succeed(
              "curl --silent --show-error --output /dev/null "
              "--data-urlencode sub=alice "
              "--cookie /tmp/login-cookies "
              "--cookie-jar /tmp/login-cookies "
              "--write-out='%header{location}' "
              f"{shlex.quote(authorization_url)}"
          ).strip()
          assert callback_url.startswith(
              "https://localhost/auth/oidc/callback?"
          ), callback_url

          native_callback = machine.succeed(
              "curl --insecure --silent --show-error --output /dev/null "
              "--cookie /tmp/login-cookies "
              "--cookie-jar /tmp/login-cookies "
              "--write-out='%header{location}' "
              f"{shlex.quote(callback_url)}"
          ).strip()
          callback = urlsplit(native_callback)
          assert callback.hostname == "127.0.0.1", native_callback
          assert callback.port == 49152, native_callback
          callback_query = parse_qs(callback.query)
          assert callback_query["user_id"] == ["1"], callback_query
          assert len(callback_query["access_token"]) == 1, callback_query

      oidc_login()
      first_profile = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed "
          "--tuples-only --no-align --field-separator='|' "
          "--command=\"SELECT users.admin, profiles.avatar_url "
          "FROM users JOIN local_user_profiles profiles "
          "ON profiles.user_id = users.id WHERE users.id = 1\""
      ).strip()
      assert first_profile == "f|", first_profile

      oidc_login()
      second_profile = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed "
          "--tuples-only --no-align --field-separator='|' "
          "--command=\"SELECT users.admin, profiles.avatar_url "
          "FROM users JOIN local_user_profiles profiles "
          "ON profiles.user_id = users.id WHERE users.id = 1\""
      ).strip()
      assert second_profile == "f|", second_profile
      assert machine.succeed(
          "runuser -u postgres -- psql --dbname=zed "
          "--tuples-only --no-align "
          "--command='SELECT count(*) FROM local_oidc_identities'"
      ).strip() == "1"

      known_token = "zed-1.16.1-contract-token"
      known_token_hash = hashlib.sha256(known_token.encode()).hexdigest()
      update_token = (
          "UPDATE local_user_tokens "
          f"SET access_token_hash = '{known_token_hash}' WHERE user_id = 1"
      )
      machine.succeed(
          "runuser -u postgres -- psql --dbname=zed --set=ON_ERROR_STOP=1 "
          f"--command={shlex.quote(update_token)}"
      )
      authenticated_user = json.loads(machine.succeed(
          "curl --insecure --silent --show-error "
          f"--header={shlex.quote(f'Authorization: 1 {known_token}')} "
          "https://localhost/client/users/me"
      ))
      assert authenticated_user["user"] == {
          "id_v2": "local-1",
          "legacy_user_id": 1,
          "metrics_id": authenticated_user["user"]["metrics_id"],
          "username": "alice",
          "avatar_url": "",
          "github_login": "alice",
          "name": "Alice Developer",
          "is_staff": False,
          "accepted_tos_at": None,
          "has_connected_to_collab_once": False,
      }, authenticated_user
      assert authenticated_user["feature_flags"] == [], authenticated_user
      assert authenticated_user["default_organization_id"] == "local"
      assert authenticated_user["configuration_by_organization"]["local"][
          "is_collaboration_enabled"
      ] is True

      internal_authorization = f"Authorization: Bearer {key}"

      def internal_request(path, body):
          return json.loads(machine.succeed(
              "curl --insecure --silent --show-error --request POST "
              "--header='Content-Type: application/json' "
              f"--header={shlex.quote(internal_authorization)} "
              f"--data={shlex.quote(json.dumps(body))} "
              f"https://localhost{path}"
          ))

      expected_internal_user = {
          "id": "local-1",
          "legacy_user_id": 1,
          "username": "alice",
          "github_login": "alice",
          "avatar_url": "",
          "name": "Alice Developer",
          "admin": False,
          "connected_once": False,
      }
      assert internal_request(
          "/internal/users/look_up_by_legacy_id",
          {"legacy_user_ids": [1, 999]},
      ) == {"users": [expected_internal_user]}
      assert internal_request(
          "/internal/users/look_up_by_github_login",
          {"github_login": "ALICE"},
      ) == {"user": expected_internal_user}
      assert internal_request(
          "/internal/users/fuzzy_search",
          {"query": "lic", "limit": 10},
      ) == {"users": [expected_internal_user]}

      machine.succeed(
          "runuser -u postgres -- psql --dbname=zed --set=ON_ERROR_STOP=1 "
          "--command=\"INSERT INTO channels (id, name, parent_path) "
          "VALUES (900001, 'contract-test', 'root'); "
          "INSERT INTO channel_members "
          "(channel_id, user_id, accepted, role) "
          "VALUES (900001, 1, true, 'member')\""
      )
      assert internal_request(
          "/internal/channel_members/fuzzy_search",
          {"channel_id": 900001, "query": "ali", "limit": 10},
      ) == {
          "channel_members": [{
              "legacy_user_id": 1,
              "kind": "member",
              "role": "member",
          }],
          "users": [expected_internal_user],
      }
    '';
  };
}

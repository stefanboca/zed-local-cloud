{
  self,
  pkgs,
}: let
  inherit
    (self.packages.${pkgs.stdenv.hostPlatform.system})
    oidc-provider-mock
    zed-collab
    zed-local-cloud
    ;

  livekitApiKey = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  livekitApiSecret = "zed-livekit-secret-0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
  livekitKeysText = ''
    unrelated-application: unrelated-secret-0123456789abcdef
    ${livekitApiKey}: ${livekitApiSecret}
  '';
  livekitKeys = pkgs.writeText "test-livekit-keys" livekitKeysText;

  websocketNotificationClient =
    pkgs.writers.writePython3 "websocket-notification-client" {
      libraries = [pkgs.python3Packages.websocket-client];
    } ''
      import ssl
      import sys
      from pathlib import Path

      import websocket

      token, ready_file = sys.argv[1:]
      socket = websocket.create_connection(
          "wss://localhost/client/users/connect",
          header=[
              f"Authorization: 1 {token}",
              "x-zed-protocol-version: 0",
          ],
          sslopt={"cert_reqs": ssl.CERT_NONE},
          timeout=10,
      )
      Path(ready_file).touch()
      message = socket.recv()
      assert isinstance(message, bytes), type(message)
      print(message.hex(), flush=True)
      socket.close()
    '';
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
        collab.package = zed-collab;
        virtualHost = "localhost";
        upstreamApiUrl = "http://127.0.0.1:18081";
        upstreamCloudUrl = "http://127.0.0.1:18081";
        livekit.apiKey = livekitApiKey;
        collab.environment.BLOB_STORE_REGION = "test-region";
        localCloud.settings.oidc = {
          admin_claim = "groups";
          admin_value = "zed-admin";
          issuer_url = "http://127.0.0.1:18080";
          client_id = "zed-local";
        };
        secrets.oidcClientSecretFile = toString (pkgs.writeText "test-oidc-client-secret" "test-client-secret");
      };
      services.livekit.keyFile = livekitKeys;

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
          "zed-provision.service",
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
          "zed-provision.service",
          "zed-local-cloud.service",
          "zed-collab.service",
      ]:
          user = machine.succeed(
              f"systemctl show {unit} --property=User --value"
          ).strip()
          assert user == "zed", f"{unit} runs as {user!r}"

      for unit in ["zed-provision.service", "zed-local-cloud.service"]:
          state_directory = machine.succeed(
              f"systemctl show {unit} --property=StateDirectory --value"
          ).strip()
          assert state_directory == "zed", (
              f"{unit} uses state directory {state_directory!r}"
          )
      assert machine.succeed(
          "systemctl show zed-collab.service --property=StateDirectory --value"
      ).strip() == ""

      zed_target_wants = set(machine.succeed(
          "systemctl show zed.target --property=Wants --value"
      ).split())
      assert zed_target_wants == {
          "zed-provision.service",
          "zed-local-cloud.service",
          "zed-collab.service",
      }, zed_target_wants
      assert machine.succeed(
          "systemctl show zed-local-cloud.service --property=Type --value"
      ).strip() == "notify"
      for unit in [
          "zed-provision.service",
          "zed-local-cloud.service",
          "zed-collab.service",
      ]:
          requires = set(machine.succeed(
              f"systemctl show {unit} --property=Requires --value"
          ).split())
          assert "postgresql.target" in requires, (unit, requires)
      collab_after = set(machine.succeed(
          "systemctl show zed-collab.service --property=After --value"
      ).split())
      assert "zed-local-cloud.service" in collab_after, collab_after
      collab_requires = set(machine.succeed(
          "systemctl show zed-collab.service --property=Requires --value"
      ).split())
      assert "zed-local-cloud.service" not in collab_requires, collab_requires
      assert "livekit.service" not in collab_requires, collab_requires
      collab_wants = set(machine.succeed(
          "systemctl show zed-collab.service --property=Wants --value"
      ).split())
      assert "zed-local-cloud.service" in collab_wants, collab_wants
      assert "livekit.service" in collab_wants, collab_wants

      assert machine.succeed(
          "stat --format='%U:%G %a' /var/lib/zed"
      ).strip() == "zed:zed 700"
      assert machine.succeed(
          "stat --format='%U:%G %a' /var/lib/zed/INTERNAL_API_KEY"
      ).strip() == "zed:zed 600"
      assert machine.succeed(
          "stat --format='%U:%G %a %s' /var/lib/zed/COOKIE_KEY"
      ).strip() == "zed:zed 600 88"
      key = machine.succeed("cat /var/lib/zed/INTERNAL_API_KEY")
      cookie_key = machine.succeed("cat /var/lib/zed/COOKIE_KEY")
      cookie_key_hash = machine.succeed(
          "sha256sum /var/lib/zed/COOKIE_KEY"
      ).split()[0]
      assert len(base64.b64decode(cookie_key, validate=True)) == 64
      assert len(base64.b64decode(key, validate=True)) == 128
      livekit_key = ${builtins.toJSON livekitApiKey}
      livekit_secret = ${builtins.toJSON livekitApiSecret}

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
      assert auth_command[-3] == "serve", auth_command
      assert auth_command[-2] == "--config", auth_command
      auth_config_text = machine.succeed(f"cat {auth_command[-1]}")
      auth_config = tomllib.loads(auth_config_text)
      assert auth_config["base_url"] == "https://localhost"
      assert auth_config["bind_address"] == "127.0.0.1:8787"
      assert "database_url" not in auth_config
      assert "internal_api_key_file" not in auth_config
      assert "client_secret_file" not in auth_config["oidc"]
      assert "redirect_url" not in auth_config["oidc"]
      assert "test-client-secret" not in auth_config_text
      assert machine.succeed(
          "cat /run/credentials/zed-local-cloud.service/OIDC_CLIENT_SECRET"
      ) == "test-client-secret"

      collab_wrapper_pid = machine.succeed(
          "systemctl show zed-collab.service --property=MainPID --value"
      ).strip()
      collab_command = machine.succeed(
          f"tr '\\0' '\\n' </proc/{collab_wrapper_pid}/cmdline"
      ).splitlines()
      assert collab_command[0].endswith("/secretspec"), collab_command
      assert "run" in collab_command, collab_command
      assert "--scope" in collab_command, collab_command
      assert "collab" in collab_command, collab_command
      collab_pid = machine.succeed(
          f"pgrep --parent {collab_wrapper_pid}"
      ).strip()
      collab_environment = dict(
          line.split("=", 1)
          for line in machine.succeed(
              f"tr '\\0' '\\n' </proc/{collab_pid}/environ"
          ).splitlines()
          if "=" in line
      )
      assert collab_environment["ZED_CLOUD_URL"] == "http://127.0.0.1:8787"
      assert collab_environment["ZED_ENVIRONMENT"] == "production"
      assert collab_environment["BLOB_STORE_REGION"] == "test-region"
      assert collab_environment["LIVEKIT_SERVER"] == "https://localhost/livekit"
      assert "ZED_CLIENT_CHECKSUM_SEED" not in collab_environment
      assert collab_environment["DATABASE_URL"] == "postgres://zed@%2Frun%2Fpostgresql/zed"
      assert collab_environment["ZED_CLOUD_INTERNAL_API_KEY"] == key
      assert collab_environment["LIVEKIT_KEY"] == livekit_key
      assert collab_environment["LIVEKIT_SECRET"] == livekit_secret
      assert "INTERNAL_API_KEY" not in collab_environment
      assert machine.succeed(
          "cat /run/credentials/livekit.service/livekit-secrets"
      ) == ${builtins.toJSON livekitKeysText}
      machine.fail("test -e /run/zed-collab/crates/collab/.env.toml")
      machine.fail("test -e /var/lib/zed/collab/.env.secret.toml")

      machine.succeed("systemctl restart zed-provision.service")
      assert machine.succeed(
          "sha256sum /var/lib/zed/COOKIE_KEY"
      ).split()[0] == cookie_key_hash
      assert machine.succeed(
          "cat /var/lib/zed/INTERNAL_API_KEY"
      ) == key
      assert machine.succeed("cat /var/lib/zed/LIVEKIT_KEY") == livekit_key
      assert machine.succeed("cat /var/lib/zed/LIVEKIT_SECRET") == livekit_secret

      migrations = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed --tuples-only --no-align "
          "--command='SELECT version FROM _sqlx_migrations ORDER BY version'"
      ).splitlines()
      assert migrations == [
          "20251208000000",
          "20260724000000",
          "20260806000000",
          "20260823000000",
          "20260902000000",
          "20260902000001",
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

      pg_trgm_is_current = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed --tuples-only --no-align "
          "--command=\"SELECT installed_version = default_version "
          "FROM pg_available_extensions WHERE name = 'pg_trgm'\""
      ).strip()
      assert pg_trgm_is_current == "t", pg_trgm_is_current

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
          "'https://localhost/releases/stable/1.17.2/asset?asset=zed-remote-server'"
      ))
      assert release_asset["path"] == (
          "/releases/stable/1.17.2/asset"
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
      assert machine.succeed(
          "runuser -u postgres -- psql --dbname=zed "
          "--tuples-only --no-align "
          "--command='SELECT count(*) FROM local_user_tokens WHERE user_id = 1'"
      ).strip() == "2"

      known_token = "zed-1.17.2-contract-token"
      known_token_hash = hashlib.sha256(known_token.encode()).hexdigest()
      insert_token = (
          "INSERT INTO local_user_tokens (user_id, access_token_hash) "
          f"VALUES (1, '{known_token_hash}')"
      )
      machine.succeed(
          "runuser -u postgres -- psql --dbname=zed --set=ON_ERROR_STOP=1 "
          f"--command={shlex.quote(insert_token)}"
      )

      machine.execute(
          "${websocketNotificationClient} "
          f"{shlex.quote(known_token)} /tmp/user-update-ready "
          ">/tmp/user-update-message 2>/tmp/user-update-error &"
      )
      machine.wait_until_succeeds("test -e /tmp/user-update-ready")
      oidc_login()
      machine.wait_until_succeeds("test -s /tmp/user-update-message")
      user_update_message = machine.succeed(
          "cat /tmp/user-update-message"
      ).strip()
      assert user_update_message == "6b5573657255706461746564", (
          user_update_message,
          machine.succeed("cat /tmp/user-update-error"),
      )

      def authenticate(token):
          return json.loads(machine.succeed(
              "curl --insecure --silent --show-error "
              f"--header={shlex.quote(f'Authorization: 1 {token}')} "
              "https://localhost/client/users/me"
          ))

      authenticated_user = authenticate(known_token)
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

      machine.succeed(
          "systemctl restart zed-local-cloud.service zed-collab.service"
      )
      machine.wait_for_unit("zed-local-cloud.service")
      machine.wait_for_unit("zed-collab.service")
      assert authenticate(known_token) == authenticated_user
      rpc_status = machine.succeed(
          "curl --insecure --silent --show-error --output /dev/null "
          f"--header={shlex.quote(f'Authorization: 1 {known_token}')} "
          "--write-out='%{http_code}' https://localhost/actual_rpc"
      ).strip()
      assert rpc_status != "401", rpc_status

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
          {"query": "ali", "limit": 10},
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

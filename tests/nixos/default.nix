{
  module,
  pkgs,
  zedLocalCloudPackage,
  zedCollabPackage,
}: {
  nixos-basic = pkgs.testers.runNixOSTest {
    name = "zed-local-cloud-basic";

    nodes.machine = {
      imports = [module];

      services.zed-local-cloud = {
        enable = true;
        package = zedLocalCloudPackage;
        virtualHost = "localhost";
        upstreamApiUrl = "http://127.0.0.1:18081";
        collab.package = zedCollabPackage;
        oidc = {
          adminClaim = "groups";
          adminValue = "zed-admin";
          issuerUrl = "http://127.0.0.1:18080";
          clientId = "zed-local";
          clientSecretFile = toString (pkgs.writeText "test-oidc-client-secret" "test-client-secret");
        };
      };

      systemd.services.fake-oidc = {
        wantedBy = ["multi-user.target"];
        before = ["zed-local-cloud.service"];
        serviceConfig.ExecStart = "${pkgs.python3}/bin/python ${../fake-oidc.py} ${pkgs.openssl}/bin/openssl";
      };
      systemd.services.fake-extensions = {
        wantedBy = ["multi-user.target"];
        before = ["caddy.service"];
        serviceConfig.ExecStart = "${pkgs.python3}/bin/python ${../fake-extensions.py}";
      };
      systemd.services.zed-local-cloud = {
        requires = ["fake-oidc.service"];
        after = ["fake-oidc.service"];
      };
    };

    testScript = ''
      import base64
      import json
      import shlex
      import tomllib
      from urllib.parse import parse_qs, quote, urlsplit

      start_all()

      for unit in [
          "postgresql.target",
          "fake-extensions.service",
          "fake-oidc.service",
          "zed-setup.service",
          "zed-local-cloud.service",
          "zed-collab.service",
          "caddy.service",
      ]:
          machine.wait_for_unit(unit)

      machine.wait_for_open_port(443)
      machine.wait_for_open_port(8787)

      failed_units = machine.succeed(
          "systemctl list-units --state=failed --no-legend --plain"
      )
      assert failed_units.strip() == "", failed_units

      for unit in [
          "zed-setup.service",
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

      assert machine.succeed(
          "stat --format='%U:%G %a' /var/lib/zed"
      ).strip() == "zed:zed 700"
      assert machine.succeed(
          "stat --format='%U:%G %a' /var/lib/zed/internal-api-key"
      ).strip() == "zed:zed 600"
      key = machine.succeed("cat /var/lib/zed/internal-api-key")
      assert len(base64.b64decode(key, validate=True)) == 128

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
      assert auth_config["base_url"] == "localhost"
      assert auth_config["bind_address"] == "127.0.0.1:8787"
      assert "browser_session_lifetime_seconds" not in auth_config
      assert auth_config["database_url"] == (
          "postgres://zed@%2Frun%2Fpostgresql/zed"
      )
      assert auth_config["internal_api_key_file"] == (
          "/var/lib/zed/internal-api-key"
      )
      assert auth_config["oidc"]["client_secret_file"] == (
          "/run/credentials/zed-local-cloud.service/oidc-client-secret"
      )
      assert "redirect_url" not in auth_config["oidc"]
      assert "test-client-secret" not in auth_config_text
      assert machine.succeed(
          "cat /run/credentials/zed-local-cloud.service/oidc-client-secret"
      ) == "test-client-secret"

      collab_environment = tomllib.loads(
          machine.succeed("cat /run/zed-collab/crates/collab/.env.toml")
      )
      assert (
          collab_environment["DATABASE_URL"]
          == "postgres://zed@%2Frun%2Fpostgresql/zed"
      )
      assert collab_environment["ZED_CLOUD_URL"] == "http://127.0.0.1:8787"
      assert collab_environment["ZED_ENVIRONMENT"] == "production"
      collab_secret = tomllib.loads(
          machine.succeed(
              "cat /run/zed-collab/crates/collab/.env.secret.toml"
          )
      )
      assert collab_secret == {"ZED_CLOUD_INTERNAL_API_KEY": key}

      machine.succeed("systemctl restart zed-setup.service")
      assert machine.succeed(
          "cat /var/lib/zed/internal-api-key"
      ) == key

      migrations = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed --tuples-only --no-align "
          "--command='SELECT version FROM _sqlx_migrations ORDER BY version'"
      ).splitlines()
      assert migrations == [
          "20251208000000",
          "20260724000000",
          "20260806000000",
          "20260807000000",
      ], migrations

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

      def oidc_login(automatic):
          signin_url = (
              "https://localhost/native_app_signin"
              "?native_app_port=49152"
              f"&native_app_public_key={quote(native_public_key)}"
          )
          if automatic:
              authorization_url = machine.succeed(
                  "curl --insecure --silent --show-error --output /dev/null "
                  "--cookie /tmp/login-cookies "
                  "--cookie-jar /tmp/login-cookies "
                  "--write-out='%header{location}' "
                  f"{shlex.quote(signin_url)}"
              ).strip()
          else:
              login_page = machine.succeed(
                  "curl --insecure --silent --show-error "
                  "--cookie-jar /tmp/login-cookies "
                  f"{shlex.quote(signin_url)}"
              )
              assert "Continue with OpenID Connect" in login_page

              authorization_url = machine.succeed(
                  "curl --insecure --silent --show-error --output /dev/null "
                  "--request POST "
                  "--cookie /tmp/login-cookies "
                  "--cookie-jar /tmp/login-cookies "
                  "--write-out='%header{location}' "
                  "https://localhost/auth/oidc/start"
              ).strip()
          assert authorization_url.startswith(
              "http://127.0.0.1:18080/authorize?"
          ), authorization_url

          callback_url = machine.succeed(
              "curl --silent --show-error --output /dev/null "
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

      oidc_login(False)
      first_profile = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed "
          "--tuples-only --no-align --field-separator='|' "
          "--command=\"SELECT users.admin, profiles.avatar_url "
          "FROM users JOIN local_user_profiles profiles "
          "ON profiles.user_id = users.id WHERE users.id = 1\""
      ).strip()
      assert first_profile == (
          "t|https://images.example.com/alice-1.png"
      ), first_profile

      oidc_login(True)
      second_profile = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed "
          "--tuples-only --no-align --field-separator='|' "
          "--command=\"SELECT users.admin, profiles.avatar_url "
          "FROM users JOIN local_user_profiles profiles "
          "ON profiles.user_id = users.id WHERE users.id = 1\""
      ).strip()
      assert second_profile == (
          "f|https://images.example.com/alice-2.png"
      ), second_profile
      assert machine.succeed(
          "runuser -u postgres -- psql --dbname=zed "
          "--tuples-only --no-align "
          "--command='SELECT count(*) FROM local_oidc_identities'"
      ).strip() == "1"
      assert machine.succeed(
          "runuser -u postgres -- psql --dbname=zed "
          "--tuples-only --no-align "
          "--command='SELECT count(*) FROM local_browser_sessions'"
      ).strip() == "1"
    '';
  };
}

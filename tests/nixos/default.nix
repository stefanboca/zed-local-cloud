{
  module,
  pkgs,
  zedAuthPackage,
  zedCollabPackage,
}: {
  nixos-basic = pkgs.testers.runNixOSTest {
    name = "zed-auth-basic";

    nodes.machine = {
      imports = [module];

      services.zed-auth = {
        enable = true;
        package = zedAuthPackage;
        virtualHost = "http://localhost";
        collab.package = zedCollabPackage;
      };
    };

    testScript = ''
      import base64
      import json
      import tomllib

      start_all()

      for unit in [
          "postgresql.target",
          "zed-setup.service",
          "zed-auth.service",
          "zed-collab.service",
          "caddy.service",
      ]:
          machine.wait_for_unit(unit)

      machine.wait_for_open_port(80)

      failed_units = machine.succeed(
          "systemctl list-units --state=failed --no-legend --plain"
      )
      assert failed_units.strip() == "", failed_units

      for unit in [
          "zed-setup.service",
          "zed-auth.service",
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
          "stat --format='%U:%G %a' /var/lib/zed/zed-auth.toml"
      ).strip() == "zed:zed 600"

      secret_toml = machine.succeed("cat /var/lib/zed/zed-auth.toml")
      secret = tomllib.loads(secret_toml)
      assert set(secret) == {"ZED_CLOUD_INTERNAL_API_KEY"}, secret
      key = secret["ZED_CLOUD_INTERNAL_API_KEY"]
      assert len(base64.b64decode(key, validate=True)) == 128

      auth_environment = machine.succeed(
          "systemctl show zed-auth.service --property=Environment --value"
      )
      assert (
          "DATABASE_URL=postgres://zed@%2Frun%2Fpostgresql/zed"
          in auth_environment
      )
      assert (
          "ZED_AUTH_SECRETS_FILE=/var/lib/zed/zed-auth.toml"
          in auth_environment
      )

      collab_environment = tomllib.loads(
          machine.succeed("cat /run/zed-collab/crates/collab/.env.toml")
      )
      assert (
          collab_environment["DATABASE_URL"]
          == "postgres://zed@%2Frun%2Fpostgresql/zed"
      )
      assert collab_environment["ZED_CLOUD_URL"] == "http://127.0.0.1:8787"
      assert collab_environment["ZED_ENVIRONMENT"] == "production"
      assert machine.succeed(
          "readlink /run/zed-collab/crates/collab/.env.secret.toml"
      ).strip() == "/var/lib/zed/zed-auth.toml"

      machine.succeed("systemctl restart zed-setup.service")
      assert machine.succeed(
          "cat /var/lib/zed/zed-auth.toml"
      ) == secret_toml

      migrations = machine.succeed(
          "runuser -u postgres -- psql --dbname=zed --tuples-only --no-align "
          "--command='SELECT version FROM _sqlx_migrations ORDER BY version'"
      ).splitlines()
      assert migrations == ["20251208000000", "20260724000000"], migrations

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
              "curl --silent --show-error --output /dev/null "
              f"--write-out='%{{http_code}} %{{redirect_url}}' "
              f"http://localhost{path}"
          ).strip()

      def location(path):
          return machine.succeed(
              "curl --silent --show-error --output /dev/null "
              f"--write-out='%header{{location}}' http://localhost{path}"
          ).strip()

      assert response("/healthz") == "204"
      assert response("/rpc") == "307 http://localhost/actual_rpc"
      assert location("/rpc") == "http://localhost/actual_rpc"
      rpc_location_with_port = machine.succeed(
          "curl --silent --show-error --output /dev/null "
          "--header='Host: localhost:18787' "
          "--write-out='%header{location}' http://localhost/rpc"
      ).strip()
      assert rpc_location_with_port == "http://localhost:18787/actual_rpc"
      assert response("/actual_rpc") == "401"
      assert response("/client/users/me") == "401"
      assert machine.succeed(
          "curl --silent --show-error --output /dev/null "
          "--request POST "
          "--header='Content-Type: application/json' "
          "--data='{\"channel_id\":1,\"query\":\"local\",\"limit\":10}' "
          "--write-out='%{http_code}' "
          "http://localhost/internal/channel_members/fuzzy_search"
      ).strip() == "401"
      assert response("/models") == "200"
      models = machine.succeed(
          "curl --silent --show-error http://localhost/models"
      )
      assert json.loads(models) == {
          "models": [],
          "default_model": None,
          "default_fast_model": None,
          "recommended_models": [],
      }
      assert response("/native_app_signin_succeeded") == (
          "302 https://zed.dev/native_app_signin_succeeded"
      )
    '';
  };
}

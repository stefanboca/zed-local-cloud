{
  config,
  lib,
  pkgs,
  ...
}: let
  inherit (lib.lists) optional;
  inherit (lib.meta) getExe;
  inherit (lib.modules) mkIf;
  inherit (lib.options) mkEnableOption mkOption mkPackageOption;
  inherit (lib.types) bool int listOf port str;

  cfg = config.services.zed-auth;

  collabEnvironment = (pkgs.formats.toml {}).generate "zed-collab.toml" {
    DATABASE_URL = cfg.database.url;
    DATABASE_MAX_CONNECTIONS = cfg.collab.databaseMaxConnections;
    HTTP_PORT = cfg.collab.port;
    LOG_JSON = cfg.collab.logJson;
    RUST_LOG = cfg.collab.logLevel;
    ZED_CLIENT_CHECKSUM_SEED = cfg.collab.clientChecksumSeed;
    ZED_CLOUD_URL = "http://${cfg.host}:${toString cfg.port}";
    ZED_ENVIRONMENT = "production";
  };

  postgresqlDependencies = optional cfg.database.createLocally "postgresql.target";

  hardening = {
    LockPersonality = true;
    MemoryDenyWriteExecute = true;
    NoNewPrivileges = true;
    PrivateDevices = true;
    PrivateTmp = true;
    ProtectClock = true;
    ProtectControlGroups = true;
    ProtectHome = true;
    ProtectHostname = true;
    ProtectKernelLogs = true;
    ProtectKernelModules = true;
    ProtectKernelTunables = true;
    ProtectSystem = "strict";
    RestrictNamespaces = true;
    RestrictRealtime = true;
    RestrictSUIDSGID = true;
    SystemCallArchitectures = "native";
    UMask = "0077";
  };
in {
  options.services.zed-auth = {
    enable = mkEnableOption "the local Zed authentication and collaboration stack";
    package = mkPackageOption pkgs "zed-auth" {};

    user = mkOption {
      type = str;
      default = "zed";
      description = "User under which the facade and collab server run.";
    };

    group = mkOption {
      type = str;
      default = "zed";
      description = "Group under which the facade and collab server run.";
    };

    host = mkOption {
      type = str;
      default = "127.0.0.1";
      description = "Address on which the facade listens.";
    };

    port = mkOption {
      type = port;
      default = 8787;
      description = "Port on which the facade listens.";
    };

    virtualHost = mkOption {
      type = str;
      example = "zed.example.com";
      description = "Caddy virtual host serving the public Zed-compatible endpoint.";
    };

    upstreamWebUrl = mkOption {
      type = str;
      default = "https://zed.dev";
      description = "Upstream origin used for native sign-in and web routes.";
    };

    upstreamApiUrl = mkOption {
      type = str;
      default = "https://api.zed.dev";
      description = "Upstream origin used for extension API routes.";
    };

    upstreamCloudUrl = mkOption {
      type = str;
      default = "https://cloud.zed.dev";
      description = "Upstream origin used to validate users and proxy Cloud API routes.";
    };

    adminGithubLogins = mkOption {
      type = listOf str;
      default = [];
      example = ["octocat"];
      description = "GitHub logins which receive administrator privileges in local collab.";
    };

    logLevel = mkOption {
      type = str;
      default = "zed_auth=info,tower_http=info,sqlx=warn";
      description = "Tracing filter used by the facade.";
    };

    collab = {
      package = mkPackageOption pkgs "zed-collab" {};

      port = mkOption {
        type = port;
        default = 8080;
        description = "Port on which the local collab server listens.";
      };

      databaseMaxConnections = mkOption {
        type = int;
        default = 5;
        description = "Maximum number of collab database connections.";
      };

      clientChecksumSeed = mkOption {
        type = str;
        default = "development-checksum-seed";
        description = "Seed used for Zed client checksum validation.";
      };

      logLevel = mkOption {
        type = str;
        default = "info";
        description = "Tracing filter used by collab.";
      };

      logJson = mkOption {
        type = bool;
        default = false;
        description = "Whether collab emits JSON logs.";
      };
    };

    database = {
      createLocally = mkOption {
        type = bool;
        default = true;
        description = "Whether to create and initialize a local PostgreSQL database.";
      };

      name = mkOption {
        type = str;
        default = "zed";
        description = "Name of the shared collab database.";
      };

      url = mkOption {
        type = str;
        default = "postgres://${cfg.user}@%2Frun%2Fpostgresql/${cfg.database.name}";
        description = "PostgreSQL URL shared by collab and the authentication facade.";
      };
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.collab.databaseMaxConnections > 0;
        message = "services.zed-auth.collab.databaseMaxConnections must be positive.";
      }
      {
        assertion = !cfg.database.createLocally || cfg.database.name == cfg.user;
        message = ''
          services.zed-auth.database.name must equal services.zed-auth.user
          when createLocally is enabled, so PostgreSQL peer authentication and
          database ownership work without a password.
        '';
      }
    ];

    users.groups.${cfg.group} = {};
    users.users.${cfg.user} = {
      isSystemUser = true;
      inherit (cfg) group;
    };

    services.postgresql = mkIf cfg.database.createLocally {
      enable = true;
      ensureDatabases = [cfg.database.name];
      ensureUsers = [
        {
          name = cfg.user;
          ensureDBOwnership = true;
        }
      ];
    };

    services.caddy = {
      enable = true;
      virtualHosts.${cfg.virtualHost}.extraConfig = ''
        @rpc path /rpc
        @collab path /actual_rpc
        @signinSucceeded path /native_app_signin_succeeded
        @models {
          method GET
          path /models
        }
        @facade {
          path /healthz /native_app_signin /internal/* /client/users/me /client/users/connect /client/system_settings /client/llm_tokens /client/feedback/*
        }
        @upstreamApi path /extensions /extensions/*
        @upstreamCloud path /client/*
        @upstreamWeb path /*

        route {
          redir @rpc {http.request.scheme}://{http.request.hostport}/actual_rpc 307
          redir @signinSucceeded ${cfg.upstreamWebUrl}/native_app_signin_succeeded 302
          respond @models `{"models":[],"default_model":null,"default_fast_model":null,"recommended_models":[]}` 200

          reverse_proxy @collab 127.0.0.1:${toString cfg.collab.port} {
            rewrite /rpc
          }
          reverse_proxy @facade ${cfg.host}:${toString cfg.port}
          reverse_proxy @upstreamApi ${cfg.upstreamApiUrl}
          reverse_proxy @upstreamCloud ${cfg.upstreamCloudUrl}
          reverse_proxy @upstreamWeb ${cfg.upstreamWebUrl}
        }
      '';
    };

    systemd.services = {
      caddy = {
        wants = ["zed-auth.service" "zed-collab.service"];
        after = ["zed-auth.service" "zed-collab.service"];
      };

      zed-auth = {
        description = "Zed local authentication facade";
        wantedBy = ["multi-user.target"];
        requires = ["zed-setup.service"];
        after = ["zed-setup.service"];

        environment = {
          BIND_ADDRESS = "${cfg.host}:${toString cfg.port}";
          DATABASE_URL = lib.replaceStrings ["%"] ["%%"] cfg.database.url;
          LOCAL_ADMIN_GITHUB_LOGINS = lib.concatStringsSep "," cfg.adminGithubLogins;
          RUST_LOG = cfg.logLevel;
          UPSTREAM_CLOUD_URL = cfg.upstreamCloudUrl;
          UPSTREAM_WEB_URL = cfg.upstreamWebUrl;
          ZED_AUTH_SECRETS_FILE = "%S/zed/zed-auth.toml";
        };

        serviceConfig =
          hardening
          // {
            ExecStart = getExe cfg.package;
            Group = cfg.group;
            Restart = "on-failure";
            RestartSec = 5;
            StateDirectory = "zed";
            StateDirectoryMode = "0700";
            User = cfg.user;
          };
      };

      zed-collab = {
        description = "Zed collaboration server";
        wantedBy = ["multi-user.target"];
        requires = ["zed-auth.service" "zed-setup.service"];
        after = ["zed-auth.service" "zed-setup.service"];

        preStart = ''
          mkdir -p "$RUNTIME_DIRECTORY/crates/collab"
          ln -sf ${collabEnvironment} "$RUNTIME_DIRECTORY/crates/collab/.env.toml"
          ln -sf "$STATE_DIRECTORY/zed-auth.toml" \
            "$RUNTIME_DIRECTORY/crates/collab/.env.secret.toml"
        '';

        serviceConfig =
          hardening
          // {
            ExecStart = "${getExe cfg.collab.package} serve collab";
            Group = cfg.group;
            Restart = "on-failure";
            RestartSec = 5;
            RuntimeDirectory = "zed-collab";
            RuntimeDirectoryMode = "0700";
            StateDirectory = "zed";
            StateDirectoryMode = "0700";
            User = cfg.user;
            WorkingDirectory = "%t/zed-collab";
          };
      };

      zed-setup = {
        description = "Initialize the local Zed services";
        requires = postgresqlDependencies;
        after = postgresqlDependencies;
        before = ["zed-auth.service" "zed-collab.service"];

        environment.DATABASE_URL = lib.replaceStrings ["%"] ["%%"] cfg.database.url;

        script = ''
          secret_file="$STATE_DIRECTORY/zed-auth.toml"
          (
            set -euo pipefail
            umask 0077
            if [ ! -s "$secret_file" ]; then
              key="$(
                ${getExe pkgs.openssl} rand -base64 128 \
                  | ${lib.getExe' pkgs.coreutils "tr"} -d '\n'
              )"
              printf 'ZED_CLOUD_INTERNAL_API_KEY = "%s"\n' "$key" >"$secret_file"
            fi
          )

          ${getExe pkgs.sqlx-cli} migrate run \
            --ignore-missing \
            --source ${cfg.collab.package}/share/zed-collab/migrations \
            --database-url "$DATABASE_URL"
        '';

        serviceConfig =
          hardening
          // {
            Group = cfg.group;
            RemainAfterExit = true;
            StateDirectory = "zed";
            StateDirectoryMode = "0700";
            Type = "oneshot";
            User = cfg.user;
          };
      };
    };
  };

  _class = "nixos";
  _file = ./nixos.nix;
}

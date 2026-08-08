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
  inherit (lib.types) bool int listOf nullOr port str;

  cfg = config.services.zed-local-cloud;

  localCloudConfig = (pkgs.formats.toml {}).generate "zed-local-cloud.toml" {
    base_url = cfg.virtualHost;
    bind_address = "127.0.0.1:${toString cfg.port}";
    database_url = cfg.database.url;
    internal_api_key_file = "/var/lib/zed/internal-api-key";
    log_level = cfg.logLevel;
    oidc =
      {
        admin_claim = cfg.oidc.adminClaim;
        admin_value = cfg.oidc.adminValue;
        client_id = cfg.oidc.clientId;
        issuer_url = cfg.oidc.issuerUrl;
        scopes = cfg.oidc.scopes;
      }
      // lib.optionalAttrs (cfg.oidc.clientSecretFile != null) {
        client_secret_file = "/run/credentials/zed-local-cloud.service/oidc-client-secret";
      };
  };

  collabEnvironment = (pkgs.formats.toml {}).generate "zed-collab.toml" {
    DATABASE_URL = cfg.database.url;
    DATABASE_MAX_CONNECTIONS = cfg.collab.databaseMaxConnections;
    HTTP_PORT = cfg.collab.port;
    LOG_JSON = cfg.collab.logJson;
    RUST_LOG = cfg.collab.logLevel;
    ZED_CLIENT_CHECKSUM_SEED = cfg.collab.clientChecksumSeed;
    ZED_CLOUD_URL = "http://127.0.0.1:${toString cfg.port}";
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
  options.services.zed-local-cloud = {
    enable = mkEnableOption "the local Zed authentication and collaboration stack";
    package = mkPackageOption pkgs "zed-local-cloud" {};

    user = mkOption {
      type = str;
      default = "zed";
      description = "User under which the local cloud and collab server run.";
    };

    group = mkOption {
      type = str;
      default = "zed";
      description = "Group under which the local cloud and collab server run.";
    };

    port = mkOption {
      type = port;
      default = 8787;
      description = "Localhost port on which the local cloud listens.";
    };

    virtualHost = mkOption {
      type = str;
      example = "zed.example.com";
      description = "Public host used as the Caddy virtual host and OIDC callback authority.";
    };

    upstreamApiUrl = mkOption {
      type = str;
      default = "https://api.zed.dev";
      description = "Upstream origin used for extension API routes.";
    };

    oidc = {
      issuerUrl = mkOption {
        type = str;
        example = "https://auth.example.com";
        description = "OIDC issuer URL.";
      };

      clientId = mkOption {
        type = str;
        example = "zed-local";
        description = "OIDC client identifier.";
      };

      clientSecretFile = mkOption {
        type = nullOr str;
        default = null;
        example = "/run/secrets/zed-oidc-client-secret";
        description = "File that contains the OIDC client secret. Use null for a public client.";
      };

      scopes = mkOption {
        type = listOf str;
        default = ["profile" "email"];
        description = "OIDC scopes in addition to the required openid scope.";
      };

      adminClaim = mkOption {
        type = str;
        default = "roles";
        description = "OIDC claim that contains the administrator value. Dot notation selects nested claims.";
      };

      adminValue = mkOption {
        type = str;
        default = "admin";
        description = "OIDC claim value that grants local administrator privileges.";
      };
    };

    logLevel = mkOption {
      type = str;
      default = "zed_local_cloud=info,sqlx=warn";
      description = "Tracing filter used by the local cloud.";
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
        description = "PostgreSQL URL shared by collab and the local cloud.";
      };
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.collab.databaseMaxConnections > 0;
        message = "services.zed-local-cloud.collab.databaseMaxConnections must be positive.";
      }
      {
        assertion = !cfg.database.createLocally || cfg.database.name == cfg.user;
        message = ''
          services.zed-local-cloud.database.name must equal services.zed-local-cloud.user
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
        route {
          redir /rpc {http.request.scheme}://{http.request.hostport}/actual_rpc 307
          handle /actual_rpc {
            rewrite * /rpc
            reverse_proxy 127.0.0.1:${toString cfg.collab.port}
          }
          @extensions path /extensions /extensions/*
          handle @extensions {
            reverse_proxy ${cfg.upstreamApiUrl} {
              header_up -Authorization
              header_up -Cookie
              header_up Host {upstream_hostport}
            }
          }
          handle {
            reverse_proxy 127.0.0.1:${toString cfg.port}
          }
        }
      '';
    };

    systemd.services = {
      caddy = {
        wants = ["zed-local-cloud.service" "zed-collab.service"];
        after = ["zed-local-cloud.service" "zed-collab.service"];
      };

      zed-local-cloud = {
        description = "Zed local cloud service";
        wantedBy = ["multi-user.target"];
        requires = ["zed-setup.service"];
        after = ["zed-setup.service"];

        serviceConfig =
          hardening
          // {
            ExecStart = "${getExe cfg.package} --config ${localCloudConfig}";
            Group = cfg.group;
            Restart = "on-failure";
            RestartSec = 5;
            StateDirectory = "zed";
            StateDirectoryMode = "0700";
            User = cfg.user;
          }
          // lib.optionalAttrs (cfg.oidc.clientSecretFile != null) {
            LoadCredential = ["oidc-client-secret:${toString cfg.oidc.clientSecretFile}"];
          };
      };

      zed-collab = {
        description = "Zed collaboration server";
        wantedBy = ["multi-user.target"];
        requires = ["zed-local-cloud.service" "zed-setup.service"];
        after = ["zed-local-cloud.service" "zed-setup.service"];

        preStart = ''
          mkdir -p "$RUNTIME_DIRECTORY/crates/collab"
          ln -sf ${collabEnvironment} "$RUNTIME_DIRECTORY/crates/collab/.env.toml"
          cat >"$RUNTIME_DIRECTORY/crates/collab/.env.secret.toml" <<EOF
          ZED_CLOUD_INTERNAL_API_KEY = "$(< "$STATE_DIRECTORY/internal-api-key")"
          EOF
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
        before = ["zed-local-cloud.service" "zed-collab.service"];

        environment.DATABASE_URL = lib.replaceStrings ["%"] ["%%"] cfg.database.url;

        script = ''
          secret_file="$STATE_DIRECTORY/internal-api-key"
          (
            set -euo pipefail
            umask 0077
            if [ ! -s "$secret_file" ]; then
              key="$(
                ${getExe pkgs.openssl} rand -base64 128 \
                  | ${lib.getExe' pkgs.coreutils "tr"} -d '\n'
              )"
              printf '%s' "$key" >"$secret_file"
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

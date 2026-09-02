{
  config,
  lib,
  pkgs,
  ...
}: let
  inherit (lib.attrsets) optionalAttrs;
  inherit (lib.lists) optional;
  inherit (lib.meta) getExe getExe';
  inherit (lib.modules) mkDefault mkIf;
  inherit (lib.options) mkEnableOption mkOption mkPackageOption;
  inherit (lib.strings) escapeShellArg optionalString;
  inherit (lib.types) bool int listOf port str;

  cfg = config.services.zed-local-cloud;

  generatedLivekitKeyFile = "/var/lib/zed/livekit-keys";
  livekitKeyFile = toString config.services.livekit.keyFile;

  localCloudConfig = (pkgs.formats.toml {}).generate "zed-local-cloud.toml" {
    base_url = "https://${cfg.virtualHost}";
    bind_address = "127.0.0.1:${toString cfg.port}";
    database_url = cfg.database.url;
    log_level = cfg.logLevel;
    oidc = {
      admin_claim = cfg.oidc.adminClaim;
      admin_value = cfg.oidc.adminValue;
      client_id = cfg.oidc.clientId;
      issuer_url = cfg.oidc.issuerUrl;
      scopes = cfg.oidc.scopes;
    };
  };

  collabEnvironment = (pkgs.formats.toml {}).generate "zed-collab.toml" ({
      DATABASE_URL = cfg.database.url;
      DATABASE_MAX_CONNECTIONS = cfg.collab.databaseMaxConnections;
      HTTP_PORT = cfg.collab.port;
      LOG_JSON = cfg.collab.logJson;
      RUST_LOG = cfg.collab.logLevel;
      ZED_CLIENT_CHECKSUM_SEED = cfg.collab.clientChecksumSeed;
      ZED_CLOUD_URL = cfg.collab.cloudUrl;
      ZED_ENVIRONMENT = "production";
    }
    // optionalAttrs cfg.livekit.enable {
      LIVEKIT_SERVER = "https://${cfg.virtualHost}/livekit";
    });

  postgresqlDependencies =
    ["zed-postgresql-extensions.service"]
    ++ optional cfg.database.createLocally "postgresql.target";

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
    migrations.package = mkPackageOption pkgs "zed-migrations" {};

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

    localCloud.enable = mkOption {
      type = bool;
      default = true;
      description = "Whether to run the packaged local cloud service. Disable this for host-side development.";
    };

    upstreamApiUrl = mkOption {
      type = str;
      default = "https://api.zed.dev";
      description = "Upstream origin used for extension API routes.";
    };

    upstreamCloudUrl = mkOption {
      type = str;
      default = "https://cloud.zed.dev";
      description = "Upstream origin used for release and cloud routes.";
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
        type = str;
        example = "/run/secrets/zed-oidc-client-secret";
        description = "File that contains the OIDC client secret.";
      };

      scopes = mkOption {
        type = listOf str;
        default = ["profile" "email" "groups"];
        description = "OIDC scopes in addition to the required openid scope.";
      };

      adminClaim = mkOption {
        type = str;
        default = "groups";
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

      cloudUrl = mkOption {
        type = str;
        default = "http://127.0.0.1:${toString cfg.port}";
        description = "Local cloud URL used by the collaboration server.";
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

    livekit.enable = mkOption {
      type = bool;
      default = true;
      description = "Whether to enable the NixOS LiveKit service and connect collab to it.";
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

    services.livekit = mkIf cfg.livekit.enable {
      enable = true;
      keyFile = mkDefault generatedLivekitKeyFile;
    };

    networking.firewall.allowedUDPPortRanges = mkIf cfg.livekit.enable [
      {
        from = config.services.livekit.settings.rtc.port_range_start;
        to = config.services.livekit.settings.rtc.port_range_end;
      }
    ];

    services.caddy = {
      enable = true;
      virtualHosts.${cfg.virtualHost}.extraConfig = ''
        route {
          ${optionalString cfg.livekit.enable ''
          handle_path /livekit/* {
            reverse_proxy 127.0.0.1:${toString config.services.livekit.settings.port}
          }
        ''}
          redir /rpc {http.request.scheme}://{http.request.hostport}/actual_rpc 307
          handle /actual_rpc {
            rewrite * /rpc
            reverse_proxy 127.0.0.1:${toString cfg.collab.port}
          }
          @upstreamApi path /extensions /extensions/*
          handle @upstreamApi {
            reverse_proxy ${cfg.upstreamApiUrl} {
              header_up -Authorization
              header_up -Cookie
              header_up Host {upstream_hostport}
            }
          }
          @releases path /releases /releases/*
          handle @releases {
            reverse_proxy ${cfg.upstreamCloudUrl}
          }
          ${optionalString cfg.localCloud.enable ''
          handle {
            reverse_proxy 127.0.0.1:${toString cfg.port}
          }
        ''}
        }
      '';
    };

    systemd.targets.zed = {
      description = "Local Zed services";
      wantedBy = ["multi-user.target"];
      wants =
        [
          "zed-credentials.service"
          "zed-collab.service"
        ]
        ++ optional cfg.localCloud.enable "zed-local-cloud.service";
    };

    systemd.services = {
      caddy = {
        wants =
          ["zed-collab.service"]
          ++ optional cfg.localCloud.enable "zed-local-cloud.service"
          ++ optional cfg.livekit.enable "livekit.service";
        after =
          ["zed-collab.service"]
          ++ optional cfg.localCloud.enable "zed-local-cloud.service"
          ++ optional cfg.livekit.enable "livekit.service";
      };

      livekit = mkIf cfg.livekit.enable {
        requires = ["zed-credentials.service"];
        after = ["zed-credentials.service"];
      };

      zed-postgresql-extensions = {
        description = "Install and update Zed PostgreSQL extensions";
        requires = optional cfg.database.createLocally "postgresql.target";
        after = optional cfg.database.createLocally "postgresql.target";
        before = [
          "zed-collab.service"
          "zed-local-cloud.service"
        ];
        serviceConfig = {
          RemainAfterExit = true;
          Type = "oneshot";
          User = cfg.user;
        };
        script = ''
          ${getExe' config.services.postgresql.finalPackage "psql"} \
            --dbname ${escapeShellArg cfg.database.url} \
            --set ON_ERROR_STOP=1 \
            --command 'CREATE EXTENSION IF NOT EXISTS pg_trgm' \
            --command 'ALTER EXTENSION pg_trgm UPDATE'
        '';
      };

      zed-local-cloud = mkIf cfg.localCloud.enable {
        description = "Zed local cloud service";
        requires = ["zed-credentials.service"] ++ postgresqlDependencies;
        after = ["zed-credentials.service"] ++ postgresqlDependencies;

        serviceConfig =
          hardening
          // {
            ExecStart = "${getExe cfg.package} --config ${localCloudConfig}";
            Group = cfg.group;
            Restart = "on-failure";
            RestartSec = 5;
            Type = "notify";
            NotifyAccess = "main";
            StateDirectory = "zed";
            StateDirectoryMode = "0700";
            User = cfg.user;
            LoadCredential = [
              "cookie-key:%S/zed/cookie-key"
              "internal-api-key:%S/zed/internal-api-key"
              "oidc-client-secret:${toString cfg.oidc.clientSecretFile}"
            ];
          };
      };

      zed-collab = {
        description = "Zed collaboration server";
        requires =
          ["zed-credentials.service"]
          ++ postgresqlDependencies;
        wants =
          optional cfg.localCloud.enable "zed-local-cloud.service"
          ++ optional cfg.livekit.enable "livekit.service";
        after =
          ["zed-credentials.service"]
          ++ postgresqlDependencies
          ++ optional cfg.localCloud.enable "zed-local-cloud.service"
          ++ optional cfg.livekit.enable "livekit.service";

        preStart = ''
          ${optionalString (!cfg.localCloud.enable) ''
            ${getExe pkgs.sqlx-cli} migrate run \
              --ignore-missing \
              --source ${cfg.migrations.package}/share/zed/migrations \
              --database-url ${escapeShellArg cfg.database.url}
          ''}

          mkdir -p "$RUNTIME_DIRECTORY/crates/collab"
          ln -sf ${collabEnvironment} "$RUNTIME_DIRECTORY/crates/collab/.env.toml"
          cat >"$RUNTIME_DIRECTORY/crates/collab/.env.secret.toml" <<EOF
          ZED_CLOUD_INTERNAL_API_KEY = "$(< "$STATE_DIRECTORY/internal-api-key")"
          EOF

          ${optionalString cfg.livekit.enable ''
            printf 'LIVEKIT_KEY = "%s"\nLIVEKIT_SECRET = "%s"\n' \
              "$(< "$CREDENTIALS_DIRECTORY/livekit-api-key")" \
              "$(< "$CREDENTIALS_DIRECTORY/livekit-api-secret")" \
              >>"$RUNTIME_DIRECTORY/crates/collab/.env.secret.toml"
          ''}
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
          }
          // optionalAttrs cfg.livekit.enable {
            LoadCredential = [
              "livekit-api-key:%S/zed/livekit-api-key"
              "livekit-api-secret:%S/zed/livekit-api-secret"
            ];
          };
      };

      zed-credentials = {
        description = "Initialize the local Zed credentials";
        script = ''
          cookie_key_file="$STATE_DIRECTORY/cookie-key"
          internal_api_key_file="$STATE_DIRECTORY/internal-api-key"
          (
            set -euo pipefail
            umask 0077
            if [ ! -s "$cookie_key_file" ]; then
              ${getExe pkgs.openssl} rand -out "$cookie_key_file" 64
            fi
            if [ ! -s "$internal_api_key_file" ]; then
              key="$(
                ${getExe pkgs.openssl} rand -base64 128 \
                  | ${getExe' pkgs.coreutils "tr"} -d '\n'
              )"
              printf '%s' "$key" >"$internal_api_key_file"
            fi
          )

          ${optionalString cfg.livekit.enable ''
            (
              set -euo pipefail
              umask 0077

              split_livekit_pair() {
                livekit_key="$(printf '%s' "''${livekit_pair%%:*}" | tr -d '[:space:]')"
                livekit_secret="$(printf '%s' "''${livekit_pair#*:}" | tr -d '[:space:]')"
                if [ "$livekit_pair" = "$livekit_secret" ]; then
                  livekit_key=""
                fi
              }

              ${optionalString (livekitKeyFile == generatedLivekitKeyFile) ''
              if [ ! -s "$STATE_DIRECTORY/livekit-api-key" ] || [ ! -s "$STATE_DIRECTORY/livekit-api-secret" ]; then
                generated="$(${getExe config.services.livekit.package} generate-keys)"
                livekit_key="$(printf '%s\n' "$generated" | sed -n 's/^API Key:[[:space:]]*//p')"
                livekit_secret="$(printf '%s\n' "$generated" | sed -n 's/^API Secret:[[:space:]]*//p')"
              else
                livekit_key="$(< "$STATE_DIRECTORY/livekit-api-key")"
                livekit_secret="$(< "$STATE_DIRECTORY/livekit-api-secret")"
              fi
            ''}
              ${optionalString (livekitKeyFile != generatedLivekitKeyFile) ''
              livekit_pair="$(sed -n '/^[[:space:]]*\(#\|$\)/!{p;q}' "$CREDENTIALS_DIRECTORY/livekit-keys")"
              split_livekit_pair
            ''}

              if [ -z "$livekit_key" ] || [ -z "$livekit_secret" ]; then
                echo "could not obtain a LiveKit API key and secret" >&2
                exit 1
              fi

              printf '%s' "$livekit_key" >"$STATE_DIRECTORY/livekit-api-key"
              printf '%s' "$livekit_secret" >"$STATE_DIRECTORY/livekit-api-secret"
              ${optionalString (livekitKeyFile == generatedLivekitKeyFile) ''
              printf '%s: %s\n' "$livekit_key" "$livekit_secret" >"$STATE_DIRECTORY/livekit-keys"
            ''}
            )
          ''}

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
          }
          // optionalAttrs (cfg.livekit.enable && livekitKeyFile != generatedLivekitKeyFile) {
            LoadCredential = ["livekit-keys:${livekitKeyFile}"];
          };
      };
    };
  };

  _class = "nixos";
  _file = ./nixos.nix;
}

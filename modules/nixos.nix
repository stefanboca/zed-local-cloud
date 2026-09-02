{
  config,
  lib,
  pkgs,
  ...
}: let
  inherit (lib.attrsets) optionalAttrs;
  inherit (lib.lists) optional;
  inherit (lib.meta) getExe;
  inherit (lib.modules) mkDefault mkIf;
  inherit (lib.options) mkEnableOption mkOption mkPackageOption;
  inherit (lib.strings) optionalString;
  inherit (lib.types) bool int listOf nullOr port str;

  cfg = config.services.zed-local-cloud;

  generatedLivekitKeyFile = "/var/lib/zed/livekit-keys";
  livekitKeyFile = toString config.services.livekit.keyFile;
  databaseUrl = "postgres://${cfg.user}@%2Frun%2Fpostgresql/${cfg.database.name}";
  databaseUrlFile =
    if cfg.database.urlFile != null
    then toString cfg.database.urlFile
    else pkgs.writeText "zed-database-url" databaseUrl;

  localCloudConfig = (pkgs.formats.toml {}).generate "zed-local-cloud.toml" {
    base_url = "https://${cfg.virtualHost}";
    bind_address = "127.0.0.1:${toString cfg.port}";
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
      DATABASE_MAX_CONNECTIONS = cfg.collab.databaseMaxConnections;
      HTTP_PORT = cfg.collab.port;
      LOG_JSON = cfg.collab.logJson;
      RUST_LOG = cfg.collab.logLevel;
      ZED_CLOUD_URL = cfg.collab.cloudUrl;
      ZED_ENVIRONMENT = "production";
    }
    // optionalAttrs (cfg.collab.clientChecksumSeed != null) {
      ZED_CLIENT_CHECKSUM_SEED = cfg.collab.clientChecksumSeed;
    }
    // optionalAttrs cfg.livekit.enable {
      LIVEKIT_SERVER = "https://${cfg.virtualHost}/livekit";
    });

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
        type = nullOr str;
        default = null;
        description = ''
          Seed used for Zed telemetry checksum validation. Leave null when
          telemetry is not collected.
        '';
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

      urlFile = mkOption {
        type = nullOr str;
        default = null;
        description = ''
          File containing the PostgreSQL URL shared by collab and the local
          cloud. Required when createLocally is false.
        '';
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
      {
        assertion = cfg.database.createLocally || cfg.database.urlFile != null;
        message = "services.zed-local-cloud.database.urlFile is required when createLocally is false.";
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
          "zed-provision.service"
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
        requires = ["zed-provision.service"];
        after = ["zed-provision.service"];
      };

      zed-local-cloud = mkIf cfg.localCloud.enable {
        description = "Zed local cloud service";
        requires = ["zed-provision.service"] ++ postgresqlDependencies;
        after = ["zed-provision.service"] ++ postgresqlDependencies;

        serviceConfig =
          hardening
          // {
            ExecStart = "${getExe cfg.package} serve --config ${localCloudConfig}";
            Group = cfg.group;
            Restart = "on-failure";
            RestartSec = 5;
            Type = "notify";
            NotifyAccess = "main";
            StateDirectory = "zed";
            StateDirectoryMode = "0700";
            User = cfg.user;
            WorkingDirectory = "${cfg.package}/share/zed-local-cloud";
            LoadCredential = [
              "DATABASE_URL:${databaseUrlFile}"
              "OIDC_CLIENT_SECRET:${toString cfg.oidc.clientSecretFile}"
            ];
            Environment = ["XDG_STATE_HOME=%S/zed"];
          };
      };

      zed-collab = {
        description = "Zed collaboration server";
        requires =
          ["zed-provision.service"]
          ++ postgresqlDependencies;
        wants =
          optional cfg.localCloud.enable "zed-local-cloud.service"
          ++ optional cfg.livekit.enable "livekit.service";
        after =
          ["zed-provision.service"]
          ++ postgresqlDependencies
          ++ optional cfg.localCloud.enable "zed-local-cloud.service"
          ++ optional cfg.livekit.enable "livekit.service";

        preStart = ''
          mkdir -p "$RUNTIME_DIRECTORY/crates/collab"
          ln -sf ${collabEnvironment} "$RUNTIME_DIRECTORY/crates/collab/.env.toml"
          ln -sf "$STATE_DIRECTORY/collab/.env.secret.toml" \
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

      zed-provision = {
        description = "Provision the local Zed services";
        requires = postgresqlDependencies;
        after = postgresqlDependencies;
        script = ''
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

          provision_args=(
            --config ${localCloudConfig}
            --collab-secret-file "$STATE_DIRECTORY/collab/.env.secret.toml"
          )
          ${optionalString cfg.livekit.enable ''
            provision_args+=(
              --livekit-key-file "$STATE_DIRECTORY/livekit-api-key"
              --livekit-secret-file "$STATE_DIRECTORY/livekit-api-secret"
            )
          ''}
          ${getExe cfg.package} provision "''${provision_args[@]}"

        '';

        serviceConfig =
          hardening
          // {
            Group = cfg.group;
            Environment = ["XDG_STATE_HOME=%S/zed"];
            LoadCredential =
              [
                "DATABASE_URL:${databaseUrlFile}"
                "OIDC_CLIENT_SECRET:${toString cfg.oidc.clientSecretFile}"
              ]
              ++ optional (cfg.livekit.enable && livekitKeyFile != generatedLivekitKeyFile) "livekit-keys:${livekitKeyFile}";
            RemainAfterExit = true;
            RuntimeDirectory = "zed-provision";
            RuntimeDirectoryMode = "0700";
            StateDirectory = "zed";
            StateDirectoryMode = "0700";
            Type = "oneshot";
            User = cfg.user;
            WorkingDirectory = "${cfg.package}/share/zed-local-cloud";
          };
      };
    };
  };

  _class = "nixos";
  _file = ./nixos.nix;
}

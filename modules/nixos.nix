{
  config,
  lib,
  pkgs,
  ...
}: let
  inherit (lib.lists) optional optionals;
  inherit (lib.meta) getExe;
  inherit (lib.modules) mkDefault mkIf;
  inherit (lib.options) mkEnableOption mkOption mkPackageOption;
  inherit (lib.strings) escapeShellArgs optionalString;
  inherit (lib.types) attrsOf bool float int listOf nullOr oneOf port str submodule;

  toml = pkgs.formats.toml {};

  databaseUrl = "postgres://${cfg.user}@%2Frun%2Fpostgresql/${cfg.database.name}";
  databaseUrlFile =
    if cfg.secrets.databaseUrlFile != null
    then toString cfg.secrets.databaseUrlFile
    else pkgs.writeText "zed-database-url" databaseUrl;

  localCloudConfig = toml.generate "zed-local-cloud.toml" cfg.localCloud.settings;
  collabEnvironment = toml.generate "zed-collab.toml" cfg.collab.environment;

  postgresqlDependencies = optional cfg.database.createLocally "postgresql.target";

  provisionArguments =
    [
      "provision"
      "--config"
      (toString localCloudConfig)
      "--collab-secret-file"
      "%S/zed/collab/.env.secret.toml"
    ]
    ++ optionals cfg.livekit.enable [
      "--livekit-api-key"
      cfg.livekit.apiKey
    ];

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

  cfg = config.services.zed-local-cloud;
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

    virtualHost = mkOption {
      type = str;
      example = "zed.example.com";
      description = "Public host used as the Caddy virtual host and OIDC callback authority.";
    };

    localCloud = {
      enable = mkOption {
        type = bool;
        default = true;
        description = "Whether to run the packaged local cloud service. Disable this for host-side development.";
      };

      settings = mkOption {
        description = "Configuration written directly to the local-cloud TOML file.";
        default = {};
        type = submodule {
          freeformType = toml.type;
          options = {
            base_url = mkOption {
              type = str;
              default = "https://${cfg.virtualHost}";
              description = "Public local-cloud URL used for OIDC callbacks.";
            };

            bind_address = mkOption {
              type = str;
              default = "127.0.0.1:8787";
              description = "Address on which local-cloud listens.";
            };

            log_level = mkOption {
              type = str;
              default = "zed_local_cloud=info,sqlx=warn";
              description = "Tracing filter used by local-cloud.";
            };

            oidc = mkOption {
              default = {};
              type = submodule {
                freeformType = toml.type;
                options = {
                  issuer_url = mkOption {
                    type = str;
                    example = "https://auth.example.com";
                    description = "OIDC issuer URL.";
                  };

                  client_id = mkOption {
                    type = str;
                    example = "zed-local";
                    description = "OIDC client identifier.";
                  };

                  scopes = mkOption {
                    type = listOf str;
                    default = ["profile" "email" "groups"];
                    description = "OIDC scopes in addition to the required openid scope.";
                  };

                  admin_claim = mkOption {
                    type = str;
                    default = "groups";
                    description = "OIDC claim containing the administrator value.";
                  };

                  admin_value = mkOption {
                    type = str;
                    default = "admin";
                    description = "OIDC claim value granting administrator privileges.";
                  };
                };
              };
            };
          };
        };
      };
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

    collab = {
      package = mkPackageOption pkgs "zed-collab" {};

      environment = mkOption {
        default = {};
        type = submodule {
          freeformType = attrsOf (oneOf [bool float int str]);
          options = {
            DATABASE_MAX_CONNECTIONS = mkOption {
              type = int;
              default = 5;
              description = "Maximum number of collab database connections.";
            };

            HTTP_PORT = mkOption {
              type = port;
              default = 8080;
              description = "Port on which collab listens.";
            };

            LIVEKIT_SERVER = mkOption {
              type = str;
              description = "Public LiveKit server URL.";
            };

            LOG_JSON = mkOption {
              type = bool;
              default = false;
              description = "Whether collab emits JSON logs.";
            };

            RUST_LOG = mkOption {
              type = str;
              default = "info";
              description = "Tracing filter used by collab.";
            };

            ZED_CLOUD_URL = mkOption {
              type = str;
              default = "http://127.0.0.1:8787";
              description = "Local cloud URL used by collab.";
            };

            ZED_ENVIRONMENT = mkOption {
              type = str;
              default = "production";
              description = "Zed deployment environment name.";
            };
          };
        };
        description = "Values written directly to collab's environment TOML file.";
      };
    };

    livekit = {
      enable = mkOption {
        type = bool;
        default = true;
        description = "Whether to enable the NixOS LiveKit service and connect collab to it.";
      };

      apiKey = mkOption {
        type = str;
        description = "API key name in services.livekit.keyFile to use for Zed.";
      };
    };

    secrets = {
      databaseUrlFile = mkOption {
        type = nullOr str;
        default = null;
        example = "/run/secrets/zed-database-url";
        description = ''
          File that contains the PostgreSQL URL for collab and the local cloud.
          This option is required when database.createLocally is false.
        '';
      };

      oidcClientSecretFile = mkOption {
        type = str;
        example = "/run/secrets/zed-oidc-client-secret";
        description = "File that contains the OIDC client secret.";
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
    };
  };

  config = mkIf cfg.enable {
    services.zed-local-cloud.collab.environment.LIVEKIT_SERVER = mkIf cfg.livekit.enable (mkDefault "https://${cfg.virtualHost}/livekit");

    assertions = [
      {
        assertion = cfg.collab.environment.DATABASE_MAX_CONNECTIONS > 0;
        message = "services.zed-local-cloud.collab.environment.DATABASE_MAX_CONNECTIONS must be positive.";
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
        assertion = cfg.database.createLocally || cfg.secrets.databaseUrlFile != null;
        message = "services.zed-local-cloud.secrets.databaseUrlFile is required when database.createLocally is false.";
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

    services.livekit.enable = mkIf cfg.livekit.enable true;
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
            reverse_proxy 127.0.0.1:${toString cfg.collab.environment.HTTP_PORT}
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
            reverse_proxy ${cfg.localCloud.settings.bind_address}
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
              "OIDC_CLIENT_SECRET:${toString cfg.secrets.oidcClientSecretFile}"
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

        serviceConfig =
          hardening
          // {
            ExecStart = "${getExe cfg.package} ${escapeShellArgs provisionArguments}";
            Group = cfg.group;
            Environment = ["XDG_STATE_HOME=%S/zed"];
            LoadCredential =
              [
                "DATABASE_URL:${databaseUrlFile}"
                "OIDC_CLIENT_SECRET:${toString cfg.secrets.oidcClientSecretFile}"
              ]
              ++ optional cfg.livekit.enable "LIVEKIT_KEYS:${toString config.services.livekit.keyFile}";
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

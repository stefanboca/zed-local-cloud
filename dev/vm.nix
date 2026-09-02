self: {
  config,
  lib,
  pkgs,
  ...
}: let
  cloudPort = 8787;
  databasePort = 5432;
  oidcPort = 9091;
  cloudOrigin = "http://localhost:${toString cloudPort}";
  livekitApiKey = "development";
  livekitKeys = pkgs.writeText "zed-development-livekit-keys" ''
    ${livekitApiKey}: development-development-development
  '';
in {
  imports = [
    self.nixosModules.default
    (import ./oidc-provider-mock.nix {
      inherit (self.packages.${pkgs.stdenv.hostPlatform.system}) oidc-provider-mock;
      port = oidcPort;
    })
  ];

  nixpkgs.overlays = [self.overlays.default];

  services.zed-local-cloud = {
    enable = true;
    localCloud.enable = false;
    virtualHost = cloudOrigin;
    collab.environment.ZED_CLOUD_URL = "http://10.0.2.2:3000";
    livekit.apiKey = livekitApiKey;
    localCloud.settings.oidc = {
      issuer_url = "http://localhost:${toString oidcPort}";
      client_id = "zed-local";
    };
    secrets.oidcClientSecretFile = toString (pkgs.writeText "zed-development-oidc-secret" "development");
  };
  services.livekit.keyFile = livekitKeys;

  services.caddy.virtualHosts.${cloudOrigin}.extraConfig = lib.mkAfter ''
    reverse_proxy 10.0.2.2:3000
  '';

  services.postgresql = {
    enableTCPIP = true;
    settings.port = databasePort;
    authentication = lib.mkAfter ''
      host zed development 0.0.0.0/0 scram-sha-256
    '';
    ensureUsers = [
      {
        name = "development";
        ensureClauses = {
          login = true;
          password = "development";
          superuser = true;
        };
      }
    ];
  };

  systemd.tmpfiles.rules = [
    "d /var/lib/zed 0700 zed zed -"
    "f+ /var/lib/zed/internal-api-key 0600 zed zed - development-development-development"
  ];

  virtualisation.vmVariant.virtualisation = {
    cores = 4;
    graphics = false;
    memorySize = 4 * 1024;
    forwardPorts = [
      {
        from = "host";
        host.port = cloudPort;
        guest.port = cloudPort;
      }
      {
        from = "host";
        host.port = databasePort;
        guest.port = databasePort;
      }
      {
        from = "host";
        host.port = oidcPort;
        guest.port = oidcPort;
      }
    ];
  };

  networking.firewall.allowedTCPPorts = [cloudPort databasePort oidcPort];
  users.users.nixos = {
    isNormalUser = true;
    initialPassword = "nixos";
    extraGroups = ["wheel"];
  };
  services.getty.autologinUser = "nixos";
  security.sudo.wheelNeedsPassword = false;
  environment.systemPackages = [pkgs.curl config.services.postgresql.package];
  system.stateVersion = "26.11";
}

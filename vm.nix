{
  lib,
  module,
  nixpkgs,
  overlay,
  system,
}:
lib.nixosSystem {
  inherit system;
  modules = [
    module
    (nixpkgs + "/nixos/modules/virtualisation/qemu-vm.nix")
    (_: {
      system.stateVersion = lib.trivial.release;

      nixpkgs.overlays = [overlay];

      services.zed-auth = {
        enable = true;
        virtualHost = "http://localhost";
      };
      services.caddy.openFirewall = true;

      virtualisation = {
        qemu.options = ["-nographic" "-serial" "mon:stdio"];
        cores = 4;
        memorySize = 4 * 1024;
        forwardPorts = [
          {
            from = "host";
            host = {
              address = "127.0.0.1";
              port = 18787;
            };
            guest.port = 80;
          }
        ];
      };

      nix.settings = {
        trusted-users = ["@wheel"];
        experimental-features = ["flakes" "nix-command"];
      };
      security.sudo.wheelNeedsPassword = false;
      users.users.nixos = {
        isNormalUser = true;
        hashedPassword = "";
        extraGroups = ["wheel"];
      };
      documentation = {
        nixos.enable = false;
        enable = false;
      };
    })
  ];
}

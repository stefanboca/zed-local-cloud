{port}: {
  lib,
  pkgs,
  ...
}: let
  inherit (lib.meta) getExe;
  users = pkgs.writeText "zed-development-oidc-users.yaml" ''
    - sub: alice
      email: alice@example.com
      email_verified: true
      preferred_username: alice
      name: Alice Developer
      groups: [developers]
    - sub: admin
      email: admin@example.com
      email_verified: true
      preferred_username: admin
      name: Admin Developer
      groups: [zed-admin]
  '';
in {
  systemd.services.oidc-provider-mock = {
    description = "Mock OpenID Connect provider";
    wantedBy = ["multi-user.target"];
    after = ["network.target"];
    serviceConfig = {
      DynamicUser = true;
      ExecStart = "${getExe pkgs.oidc-provider-mock} --host 0.0.0.0 --port ${toString port} --require-nonce true --user-claims-file ${users}";
      Restart = "on-failure";
      RestartSec = 1;
    };
  };
}

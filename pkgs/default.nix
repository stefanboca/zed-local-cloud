pkgs: {
  oidc-provider-mock = pkgs.callPackage ./oidc-provider-mock {};
  zed-collab = pkgs.callPackage ./zed-collab {};
  zed-local-cloud = pkgs.callPackage ./zed-local-cloud {};
  zed-migrations = pkgs.callPackage ./zed-migrations {};
}

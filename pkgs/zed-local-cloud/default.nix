{
  lib,
  openssl,
  pkg-config,
  rustPlatform,
  tailwindcss_4,
  topcoat-cli,
}: let
  inherit (lib.fileset) fileFilter toSource unions;
  inherit (lib.strings) hasPrefix;
in
  rustPlatform.buildRustPackage {
    pname = "zed-local-cloud";
    version = "0.1.0";

    src = toSource {
      root = ../..;
      fileset = unions [
        (fileFilter (file: file.hasExt "rs") ../..)
        (fileFilter (file: hasPrefix "Cargo" file.name) ../..)
        ../../migrations
      ];
    };

    cargoLock.lockFile = ../../Cargo.lock;
    nativeBuildInputs = [pkg-config tailwindcss_4 topcoat-cli];
    buildInputs = [openssl];
    TAILWIND_CLI = lib.getExe tailwindcss_4;

    postBuild = ''
      topcoat asset bundle --release --bin zed-local-cloud --out target/assets
    '';

    postInstall = ''
      mkdir -p $out/bin/assets
      cp -r target/assets/. $out/bin/assets/
    '';

    meta = {
      description = "Local cloud and OIDC service for Zed collab";
      mainProgram = "zed-local-cloud";
      platforms = lib.platforms.linux;
    };
  }

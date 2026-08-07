{
  lib,
  openssl,
  pkg-config,
  rustPlatform,
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
    cargoBuildFlags = ["--bin=zed-local-cloud"];

    nativeBuildInputs = [pkg-config];
    buildInputs = [openssl];

    meta = {
      description = "Local cloud and OIDC service for Zed collab";
      mainProgram = "zed-local-cloud";
      platforms = lib.platforms.linux;
    };
  }

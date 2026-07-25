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
    pname = "zed-auth";
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

    nativeBuildInputs = [pkg-config];
    buildInputs = [openssl];

    meta = {
      description = "Local authentication and Cloud API facade for Zed collab";
      mainProgram = "zed-auth";
      platforms = lib.platforms.linux;
    };
  }

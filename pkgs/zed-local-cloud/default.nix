{
  lib,
  openssl,
  pkg-config,
  rustPlatform,
  stdenv,
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
    env.TAILWIND_CLI = lib.getExe tailwindcss_4;

    # Scan the artifact built by buildRustPackage instead of making Topcoat run
    # a second Cargo build.
    postBuild = ''
      topcoat asset bundle --artifact "target/${stdenv.targetPlatform.rust.cargoShortTarget}/$cargoBuildType/zed-local-cloud"
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

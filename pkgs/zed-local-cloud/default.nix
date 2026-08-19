{
  lib,
  rustPlatform,
  tailwindcss_4,
  topcoat-cli,
}: let
  inherit (lib.fileset) fileFilter toSource unions;
  inherit (lib.meta) getExe;
  inherit (lib.strings) hasPrefix;
in
  rustPlatform.buildRustPackage {
    pname = "zed-local-cloud";
    version = "0.1.0";

    src = toSource {
      root = ../..;
      fileset = unions [
        (fileFilter (file: file.hasExt "rs" || file.hasExt "svg") ../..)
        (fileFilter (file: hasPrefix "Cargo" file.name) ../..)
        ../../.sqlx
        ../../migrations
      ];
    };

    cargoLock.lockFile = ../../Cargo.lock;
    nativeBuildInputs = [tailwindcss_4 topcoat-cli];
    env = {
      SQLX_OFFLINE = "true";
      TAILWIND_CLI = getExe tailwindcss_4;
    };

    # Let Topcoat own the Cargo build so it can bundle the exact artifact it
    # produced without launching a second build.
    buildPhase = ''
      runHook preBuild
      topcoat asset bundle --release --bin zed-local-cloud
      runHook postBuild
    '';

    checkPhase = ''
      runHook preCheck
      cargo test -j "$NIX_BUILD_CORES" --offline --profile release
      runHook postCheck
    '';

    installPhase = ''
      runHook preInstall
      install -Dm755 target/release/zed-local-cloud $out/bin/zed-local-cloud
      cp -r target/release/assets $out/bin/assets
      install -Dm644 assets/zed.svg $out/share/zed-local-cloud/zed.svg
      runHook postInstall
    '';

    meta = {
      description = "Local cloud and OIDC service for Zed collab";
      mainProgram = "zed-local-cloud";
      platforms = lib.platforms.linux;
    };
  }

{
  lib,
  openssl,
  pkg-config,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  pname = "zed-auth";
  version = "0.1.0";

  src = lib.cleanSource ../..;

  cargoLock.lockFile = ../../Cargo.lock;

  nativeBuildInputs = [pkg-config];
  buildInputs = [openssl];

  meta = {
    description = "Local authentication and Cloud API facade for Zed collab";
    license = lib.licenses.agpl3Only;
    mainProgram = "zed-auth";
    platforms = lib.platforms.linux;
  };
}

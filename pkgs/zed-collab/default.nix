{
  cmake,
  openssl,
  pkg-config,
  protobuf,
  rustPlatform,
  stdenv,
  zed-editor,
}:
rustPlatform.buildRustPackage {
  pname = "zed-collab";
  inherit (zed-editor) version;
  inherit (zed-editor) src;

  inherit (zed-editor) cargoDeps;

  patches = [./zed-cloud-url.patch];

  postPatch = ''
    rm .cargo/config.toml
  '';

  nativeBuildInputs = [
    cmake
    pkg-config
    protobuf
  ];
  buildInputs = [openssl];

  dontUseCmakeConfigure = true;

  cargoBuildFlags = ["--package=collab"];
  doCheck = false;

  installPhase = ''
    runHook preInstall

    install -Dm755 \
      target/${stdenv.hostPlatform.rust.cargoShortTarget}/release/collab \
      $out/bin/collab
    mkdir -p $out/share/zed-collab
    cp -r ${../../migrations} $out/share/zed-collab/migrations

    runHook postInstall
  '';

  meta =
    zed-editor.meta
    // {
      description = "Collaboration server from the Zed source distribution";
      mainProgram = "collab";
    };
}

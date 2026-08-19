{
  fetchCrate,
  lib,
  rustPlatform,
}:
rustPlatform.buildRustPackage rec {
  pname = "topcoat-cli";
  version = "0.6.2";

  src = fetchCrate {
    inherit pname version;
    hash = "sha256-+6DKc2yjKwju64iokWV1EUXf62GGj1W32kp7E7WVKdM=";
  };

  cargoHash = "sha256-CDnvF0CMkpjIK0dDhuxIfpm0PnyEjFvbIjWhgSfFk1w=";

  # The published crate has a doctest that imports its workspace-only
  # `topcoat` development dependency.
  doCheck = false;

  meta = {
    description = "Command-line development tools for Topcoat";
    homepage = "https://github.com/tokio-rs/topcoat";
    license = lib.licenses.mit;
    mainProgram = "topcoat";
  };
}

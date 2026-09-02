{
  lib,
  pkgs,
  ...
}: let
  inherit (lib.meta) getExe;
in {
  projectRootFile = "flake.nix";
  settings.formatter.topcoat = {
    command = getExe pkgs.topcoat-cli;
    options = ["fmt"];
    includes = ["*.rs"];
  };
  settings.formatter.sqruff.excludes = [
    # these migrations were already applied in prod at the time sqlruff
    # formatting was added.
    "migrations/20251208000000_test_schema.sql"
    "migrations/20260724000000_local_cloud.sql"
  ];
  programs = {
    # keep-sorted start block=yes
    alejandra.enable = true;
    deadnix.enable = true;
    keep-sorted.enable = true;
    rustfmt.enable = true;
    sqruff.enable = true;
    statix.enable = true;
    # keep-sorted end
  };
}

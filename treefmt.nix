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
  programs = {
    # keep-sorted start block=yes
    alejandra.enable = true;
    deadnix.enable = true;
    keep-sorted.enable = true;
    rustfmt.enable = true;
    statix.enable = true;
    # keep-sorted end
  };
}

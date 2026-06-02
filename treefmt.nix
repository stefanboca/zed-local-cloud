_: {
  projectRootFile = "flake.nix";
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

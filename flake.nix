{
  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.xz";

    treefmt-nix.url = "github:numtide/treefmt-nix";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    treefmt-nix,
    ...
  }: let
    inherit (nixpkgs) lib;
    inherit (lib.attrsets) genAttrs mapAttrs' nameValuePair;
    inherit (lib.meta) getExe;
    inherit (lib.strings) makeLibraryPath;

    systems = ["x86_64-linux" "aarch64-linux"];
    forAllSystems = genAttrs systems;
    nixpkgsFor = forAllSystems (system:
      import nixpkgs {
        inherit system;
        overlays = [self.overlays.default];
      });
    treefmtFor = forAllSystems (system: treefmt-nix.lib.evalModule nixpkgsFor.${system} ./treefmt.nix);
  in {
    packages = forAllSystems (system: import ./pkgs nixpkgsFor.${system});
    overlays.default = _: import ./pkgs;

    nixosModules = {
      default = import ./modules/nixos.nix;
    };

    devShells = forAllSystems (system: let
      pkgs = nixpkgsFor.${system};
    in {
      default = pkgs.mkShellNoCC {
        packages = [pkgs.sqlx-cli pkgs.tailwindcss_4 self.packages.${system}.topcoat-cli];
        nativeBuildInputs = with pkgs; [pkg-config];
        buildInputs = with pkgs; [openssl];

        env.LD_LIBRARY_PATH = makeLibraryPath [pkgs.openssl];
        env.TAILWIND_CLI = getExe pkgs.tailwindcss_4;
      };
    });

    formatter = forAllSystems (system: treefmtFor.${system}.config.build.wrapper);
    checks = forAllSystems (system: let
      pkgs = nixpkgsFor.${system};
      packages = mapAttrs' (n: nameValuePair "package-${n}") self.packages.${system};
      devShells = mapAttrs' (n: nameValuePair "devShell-${n}") self.devShells.${system};
      formatting = {formatting = treefmtFor.${system}.config.build.check self;};
      nixos = import ./tests/nixos {
        inherit pkgs;
        module = self.nixosModules.default;
        zedLocalCloudPackage = self.packages.${system}.zed-local-cloud;
        zedCollabPackage = self.packages.${system}.zed-collab;
      };
    in
      packages // devShells // formatting // nixos);
  };
}

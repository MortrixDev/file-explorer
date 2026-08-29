{
  description = "Terminal file explorer.";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f:
        nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        file-explorer = pkgs.callPackage ./package.nix { };
        default = file-explorer;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rust-analyzer
            clippy
            rustfmt
          ];
        };
      });

      overlays.default = final: _prev: {
        file-explorer = final.callPackage ./package.nix { };
      };

      formatter = forAllSystems (pkgs: pkgs.nixpkgs-fmt);
    };
}

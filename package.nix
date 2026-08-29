{ lib, rustPlatform }:

let
  manifest = (lib.importTOML ./Cargo.toml).package;
in
rustPlatform.buildRustPackage {
  pname = manifest.name;
  inherit (manifest) version;

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
      ./shells
    ];
  };

  cargoLock.lockFile = ./Cargo.lock;

  meta = {
    description = "Terminal file explorer with a preview pane";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "file-explorer";
  };
}

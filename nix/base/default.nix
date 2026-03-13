{inputs, ...}: {
  perSystem = {system, ...}: let
    pkgs = import inputs.nixpkgs {inherit system;};
    inherit (pkgs) lib;
    craneLib = inputs.crane.mkLib pkgs;
    workspaceRoot = ../../.;
    src = craneLib.cleanCargoSource workspaceRoot;

    commonArgs = {
      inherit src;
      strictDeps = true;
      buildInputs = lib.optionals pkgs.stdenv.isDarwin [pkgs.libiconv];
    };

    cargoArtifacts = craneLib.buildDepsOnly commonArgs;

    individualCrateArgs =
      commonArgs
      // {
        inherit cargoArtifacts;
        inherit (craneLib.crateNameFromCargoToml {inherit src;}) version;
        # Tests run via cargo-nextest
        doCheck = false;
      };

    fileSetForCrate = crate:
      lib.fileset.toSource {
        root = workspaceRoot;
        fileset = lib.fileset.unions [
          ../../Cargo.toml
          ../../Cargo.lock
          (craneLib.fileset.commonCargoSources ../../crates/phaneron-core)
          (craneLib.fileset.commonCargoSources ../../crates/my-workspace-hack)
          (craneLib.fileset.commonCargoSources crate)
        ];
      };
  in {
    _module.args = {
      inherit
        pkgs
        lib
        craneLib
        src
        commonArgs
        cargoArtifacts
        individualCrateArgs
        fileSetForCrate
        ;
    };
  };
}

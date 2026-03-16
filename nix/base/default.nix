{inputs, ...}: {
  perSystem = {system, ...}: let
    pkgs = import inputs.nixpkgs {
      inherit system;
      overlays = [
        inputs.rust-overlay.overlays.default
      ];
    };

    inherit (pkgs) lib;

    toolchain =
      pkgs.rust-bin.selectLatestNightlyWith
      (toolchain:
        toolchain.default.override {
          extensions = [
            "rust-src"
            "clippy"
            "rust-analyzer"
            "llvm-tools-preview"
            "rustfmt"
          ];
        });

    craneLib = (inputs.crane.mkLib pkgs).overrideToolchain toolchain;
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
          (craneLib.fileset.commonCargoSources ../../crates/lethe-core)
          (craneLib.fileset.commonCargoSources ../../crates/lethe-workspace-hack)
          (craneLib.fileset.commonCargoSources crate)
        ];
      };
  in {
    _module.args = {
      inherit
        pkgs
        lib
        toolchain
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

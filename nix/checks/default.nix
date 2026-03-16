{inputs, ...}: {
  perSystem = {
    config,
    pkgs,
    lib,
    craneLib,
    commonArgs,
    cargoArtifacts,
    src,
    inputs',
    ...
  }: let
    advisoryDb = inputs.advisory-db;
  in {
    checks = {
      # Build the crates as part of `nix flake check` for convenience.
      inherit (config.packages) lethe-cli lethe-core;

      lethe-workspace-clippy = craneLib.cargoClippy (
        commonArgs
        // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        }
      );

      lethe-workspace-doc = craneLib.cargoDoc (
        commonArgs
        // {
          inherit cargoArtifacts;
          env.RUSTDOCFLAGS = "--deny warnings";
        }
      );

      lethe-workspace-audit = craneLib.cargoAudit {
        inherit src;
        advisory-db = advisoryDb;
      };

      lethe-workspace-deny = craneLib.cargoDeny {
        inherit src;
      };

      lethe-workspace-nextest = craneLib.cargoNextest (
        commonArgs
        // {
          inherit cargoArtifacts;
          partitions = 1;
          partitionType = "count";
          cargoNextestPartitionsExtraArgs = "--no-tests=pass";
        }
      );

      lethe-workspace-hakari = craneLib.mkCargoDerivation {
        inherit src;
        pname = "lethe-workspace-hakari";
        cargoArtifacts = null;
        doInstallCargoArtifacts = false;

        buildPhaseCargoCommand = ''
          cargo hakari generate --diff
          cargo hakari manage-deps --dry-run
          cargo hakari verify
        '';

        nativeBuildInputs = [
          pkgs.cargo-hakari
        ];
      };
    };
  };
}

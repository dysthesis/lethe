{ inputs, ... }:
{
  perSystem =
    { config, pkgs, lib, craneLib, commonArgs, cargoArtifacts, src, inputs', ... }:
    let
      advisoryDb = inputs.advisory-db;
    in
    {
      checks = {
        # Build the crates as part of `nix flake check` for convenience.
        inherit (config.packages) phaneron-cli phaneron-core;

        phaneron-workspace-clippy = craneLib.cargoClippy (
          commonArgs
          // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          }
        );

        phaneron-workspace-doc = craneLib.cargoDoc (
          commonArgs
          // {
            inherit cargoArtifacts;
            env.RUSTDOCFLAGS = "--deny warnings";
          }
        );

        phaneron-workspace-fmt = craneLib.cargoFmt {
          inherit src;
        };

        phaneron-workspace-toml-fmt = craneLib.taploFmt {
          src = pkgs.lib.sources.sourceFilesBySuffices src [ ".toml" ];
        };

        phaneron-workspace-audit = craneLib.cargoAudit {
          inherit src;
          advisory-db = advisoryDb;
        };

        phaneron-workspace-deny = craneLib.cargoDeny {
          inherit src;
        };

        phaneron-workspace-nextest = craneLib.cargoNextest (
          commonArgs
          // {
            inherit cargoArtifacts;
            partitions = 1;
            partitionType = "count";
            cargoNextestPartitionsExtraArgs = "--no-tests=pass";
          }
        );

        phaneron-workspace-hakari = craneLib.mkCargoDerivation {
          inherit src;
          pname = "phaneron-workspace-hakari";
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

{inputs, ...}: {
  perSystem = {
    config,
    pkgs,
    lib,
    craneLib,
    commonArgs,
    cargoArtifacts,
    workspaceRoot,
    src,
    inputs',
    ...
  }: let
    advisoryDb = inputs.advisory-db;
    nixSrc = lib.cleanSourceWith {
      src = workspaceRoot;
      filter = path: _type: let
        rel = lib.removePrefix (toString workspaceRoot + "/") (toString path);
        isExcluded = lib.any (prefix: lib.hasPrefix prefix rel) [
          ".git"
          ".direnv"
          "target"
          "result"
        ];
      in
        !isExcluded;
    };
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

      lethe-workspace-deadnix =
        pkgs.runCommand "lethe-workspace-deadnix" {
          nativeBuildInputs = [pkgs.deadnix];
        } ''
          deadnix --fail ${nixSrc}
          mkdir -p $out
        '';

      lethe-workspace-statix =
        pkgs.runCommand "lethe-workspace-statix" {
          nativeBuildInputs = [pkgs.statix];
        } ''
          statix check ${nixSrc}
          mkdir -p $out
        '';

      lethe-workspace-llvm-cov = craneLib.mkCargoDerivation {
        inherit src cargoArtifacts;
        pname = "lethe-workspace-llvm-cov";
        doInstallCargoArtifacts = false;
        nativeBuildInputs = [pkgs.cargo-llvm-cov];
        buildPhaseCargoCommand = ''
          cargo llvm-cov --workspace --all-targets --locked --lcov --output-path lcov.info
        '';
        installPhase = ''
          mkdir -p $out
          cp lcov.info $out/
        '';
      };
    };
  };
}

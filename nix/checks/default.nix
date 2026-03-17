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
    ...
  }: let
    advisoryDb = inputs.advisory-db;
    llvmCovThresholds = {
      lethe-core = {
        line = 75;
        branch = 75;
        function = 75;
        region = 75;
      };
      lethe-cli = {
        line = 75;
        branch = 75;
        function = 75;
        region = 75;
      };
    };
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
        nativeBuildInputs = [pkgs.cargo-llvm-cov pkgs.python3];
        buildPhaseCargoCommand = let
          mkCovCommand = name: thresholds: let
            line = toString thresholds.line;
            branch = toString thresholds.branch;
            function = toString thresholds.function;
            region = toString thresholds.region;
            pkg = lib.escapeShellArg name;
            lcovPath = "lcov-${name}.info";
          in ''
            echo "Running cargo llvm-cov for ${name}"
            cargo llvm-cov -p ${pkg} --all-targets --locked --branch \
              --fail-under-lines ${line} \
              --fail-under-functions ${function} \
              --fail-under-regions ${region} \
              --lcov --output-path ${lcovPath}
            python check-branch-coverage.py ${lcovPath} ${branch} ${pkg}
          '';
          covCommands = lib.concatStringsSep "\n" (lib.mapAttrsToList mkCovCommand llvmCovThresholds);
        in ''
          set -euo pipefail
          cat > check-branch-coverage.py <<'PY'
          import sys

          path = sys.argv[1]
          threshold = float(sys.argv[2])
          crate = sys.argv[3]

          branches_found = 0
          branches_hit = 0

          with open(path, "r", encoding="utf-8") as handle:
              for raw in handle:
                  if raw.startswith("BRF:"):
                      branches_found += int(raw.split(":", 1)[1])
                  elif raw.startswith("BRH:"):
                      branches_hit += int(raw.split(":", 1)[1])

          if branches_found == 0:
              coverage = 100.0
          else:
              coverage = (branches_hit * 100.0) / branches_found

          if coverage + 1e-9 < threshold:
              print(
                  f"Branch coverage {coverage:.2f}% below threshold {threshold:.2f}% for {crate}"
              )
              sys.exit(1)

          print(f"Branch coverage {coverage:.2f}% (threshold {threshold:.2f}%) for {crate}")
          PY
          ${covCommands}
        '';
        installPhase = ''
          mkdir -p $out/coverage
          cp lcov-*.info $out/coverage/
        '';
      };
    };
  };
}

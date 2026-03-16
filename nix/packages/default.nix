{
  perSystem = {
    craneLib,
    individualCrateArgs,
    fileSetForCrate,
    ...
  }: let
    lethe-core = craneLib.buildPackage (
      individualCrateArgs
      // {
        pname = "lethe-core";
        cargoExtraArgs = "-p lethe-core";
        src = fileSetForCrate ../../crates/lethe-core;
      }
    );

    lethe-cli = craneLib.buildPackage (
      individualCrateArgs
      // {
        pname = "lethe-cli";
        cargoExtraArgs = "-p lethe-cli";
        src = fileSetForCrate ../../crates/lethe-cli;
        meta = {
          mainProgram = "lethe-cli";
        };
      }
    );
  in {
    packages = {
      inherit lethe-core lethe-cli;
    };
  };
}

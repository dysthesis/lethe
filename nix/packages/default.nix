{ ... }:
{
  perSystem =
    { craneLib, individualCrateArgs, fileSetForCrate, ... }:
    let
      phaneron-core = craneLib.buildPackage (
        individualCrateArgs
        // {
          pname = "phaneron-core";
          cargoExtraArgs = "-p phaneron-core";
          src = fileSetForCrate ../../crates/phaneron-core;
        }
      );

      phaneron-cli = craneLib.buildPackage (
        individualCrateArgs
        // {
          pname = "phaneron-cli";
          cargoExtraArgs = "-p phaneron-cli";
          src = fileSetForCrate ../../crates/phaneron-cli;
          meta = {
            mainProgram = "phaneron-cli";
          };
        }
      );
    in
    {
      packages = {
        inherit phaneron-core phaneron-cli;
      };
    };
}

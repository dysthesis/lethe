{ ... }:
{
  perSystem =
    { config, ... }:
    {
      apps = {
        phaneron-cli = {
          type = "app";
          program = "${config.packages.phaneron-cli}/bin/phaneron-cli";
        };
      };
    };
}

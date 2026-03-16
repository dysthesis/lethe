{...}: {
  perSystem = {config, ...}: {
    apps = {
      lethe-cli = {
        type = "app";
        program = "${config.packages.lethe-cli}/bin/lethe-cli";
      };
    };
  };
}

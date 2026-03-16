{
  perSystem = {
    config,
    craneLib,
    pkgs,
    ...
  }: {
    devShells.default = craneLib.devShell {
      inherit (config) checks;
      packages = with pkgs; [
        # Nix
        nixd
        alejandra
        statix
        deadnix
        config.treefmt.build.wrapper

        # Rust
        bacon
        cargo-hakari
        cargo-nextest
        cargo-llvm-cov
        cargo-llvm-lines
        cargo-expand

        # Miscellaneous
        git-bug
      ];
      shellHook = ''
         echo
            ${pkgs.lib.getExe pkgs.git-bug} bug
        echo
      '';
    };
  };
}

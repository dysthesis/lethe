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

        # Rust
        cargo-hakari

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

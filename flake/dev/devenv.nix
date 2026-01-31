{ lib, config, inputs, ... }: let
  inherit (lib) mkForce;
in {
  imports = with inputs; [
    devenv.flakeModule
  ];

  perSystem = { config, pkgs, ... }: let
    mkRust = pkgs: (inputs.rust-overlay.lib.mkRustBin { } pkgs)
      .stable.latest.default
      .override {
        extensions = ["rust-src"];
      };
    rust' = mkRust pkgs;
  in {
    devenv.shells.default = {
      git-hooks.hooks = {
        clippy.enable = true;
        rustfmt.enable = true;
      };
      git-hooks.tools = {
        cargo = mkForce rust';
        clippy = mkForce rust';
        rustfmt = mkForce rust';
      };

      languages.rust = {
        enable = true;
        toolchainPackage = rust';
      };
    };
  };
}

{ lib, config, inputs, ... }: {
  perSystem = { config, pkgs, ... }: let
    myChecks = pkgs.callPackage ../checks {
      inherit (inputs) crane rust-overlay;
    };
  in {
    checks = {
      inherit (myChecks) cargo-test;
    };
  };
}

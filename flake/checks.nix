{ lib, config, inputs, ... }: let
  inherit (lib) filterAttrs isDerivation licenses;
  onlyDrvs = filterAttrs (_: isDerivation);
in {
  perSystem = { config, pkgs, ... }: let
    myChecks = pkgs.callPackage ../checks.nix {
      inherit (inputs) crane rust-overlay;
    };
  in {
    checks = onlyDrvs myChecks;
  };
}

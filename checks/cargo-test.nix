{ lib, craneLib }: let
  inherit (lib.fileset) toSource unions;

  commonAttrs = {
    src = toSource {
      root = ../.;
      fileset = unions [
        ../Cargo.lock
        ../Cargo.toml
        ../src
      ];
    };
    strictDeps = true;
  };
in craneLib.cargoTest (commonAttrs // {
  cargoArtifacts = craneLib.buildDepsOnly commonAttrs;
})

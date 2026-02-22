{
  lib,
  crane,
  pkgs,
  rust-overlay,
}:

let
  inherit (lib.fileset) toSource unions;

  mkRust = pkgs: (rust-overlay.lib.mkRustBin { } pkgs)
    .stable.latest.default;

  craneLib = (crane.mkLib pkgs).overrideToolchain mkRust;

  project-src = toSource {
    root = ./.;
    fileset = unions [
      ./Cargo.lock
      ./Cargo.toml
      ./src
    ];
  };
  project-deps = craneLib.buildDepsOnly {
    src = project-src;
  };
in {
  cargo-clippy = craneLib.cargoClippy {
    src = project-src;
    cargoArtifacts = project-deps;

    cargoClippyExtraArgs = "--all-targets -- --deny warnings";
  };

  cargo-doc = craneLib.cargoDoc {
    src = project-src;
    cargoArtifacts = project-deps;
  };

  cargo-test = craneLib.cargoTest {
    src = project-src;
    cargoArtifacts = project-deps;
  };
}

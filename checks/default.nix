{
  crane,
  extend,
  rust-overlay,
}:

let
  mkRust = pkgs: (rust-overlay.lib.mkRustBin { } pkgs)
    .stable.latest.default;

  pkgs' = extend (final: prev: {
    craneLib = (crane.mkLib prev).overrideToolchain mkRust;
  });
in {
  cargo-test = pkgs'.callPackage ./cargo-test.nix { };
}

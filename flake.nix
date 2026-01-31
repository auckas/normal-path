{
  description = "Normalize paths without I/O";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";

    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";

    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs = inputs: inputs.flake-parts.lib.mkFlake { inherit inputs; } {
    imports = with inputs; [
      flake-parts.flakeModules.partitions
      ./flake/checks.nix
    ];

    systems = ["x86_64-linux"];

    partitionedAttrs.devShells = "dev";
    partitions.dev.extraInputsFlake = ./flake/dev;
    partitions.dev.module.imports = [
      ./flake/dev/devenv.nix
    ];
  };
}

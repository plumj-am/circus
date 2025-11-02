{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    nixpkgs,
    crane,
    ...
  }: let
    # FIXME: allow multi-system when I can be arsed to write the abstractions
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    craneLib = crane.mkLib pkgs;
    src = craneLib.cleanCargoSource ./.;

    commonArgs = {
      pname = "feel-ci";
      inherit src;
      strictDeps = true;
    };

    cargoArtifacts = craneLib.buildDepsOnly commonArgs;

    # Build individual workspace members
    server = craneLib.buildPackage (commonArgs
      // {
        inherit cargoArtifacts;
        pname = "server";
        cargoExtraArgs = "--package server";
      });

    evaluator = craneLib.buildPackage (commonArgs
      // {
        inherit cargoArtifacts;
        pname = "evaluator";
        cargoExtraArgs = "--package evaluator";
      });

    queue-runner = craneLib.buildPackage (commonArgs
      // {
        inherit cargoArtifacts;
        pname = "queue-runner";
        cargoExtraArgs = "--package queue-runner";
      });

    common = craneLib.buildPackage (commonArgs
      // {
        inherit cargoArtifacts;
        pname = "common";
        cargoExtraArgs = "--package common";
      });
  in {
    packages.${system} = {
      inherit server evaluator queue-runner common;
    };

    devShells.${system}.default = craneLib.devShell {
      name = "fc";
      inputsFrom = [server];
      packages = with pkgs; [
        rust-analyzer
        postgresql
        pkg-config
        openssl
      ];
    };
  };
}

{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs?ref=nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    nixpkgs,
    crane,
    self,
    ...
  }: let
    inherit (nixpkgs) lib;
    forAllSystems = lib.genAttrs ["x86_64-linux" "aarch64-linux"];
  in {
    # NixOS module for feel-ci
    nixosModules = {
      fc-ci = ./nix/modules/nixos.nix;
      default = self.nixosModules.fc-ci;
    };

    packages = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      craneLib = crane.mkLib pkgs;

      src = let
        fs = lib.fileset;
        s = ./.;
      in
        fs.toSource {
          root = s;
          fileset = fs.unions [
            (s + /crates)
            (s + /Cargo.lock)
            (s + /Cargo.toml)
          ];
        };

      commonArgs = {
        pname = "feel-ci";
        inherit src;
        strictDeps = true;
        nativeBuildInputs = with pkgs; [pkg-config];
        buildInputs = with pkgs; [openssl];
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in {
      demo-vm = pkgs.callPackage ./nix/demo-vm.nix {
        nixosModule = self.nixosModules.default;
        fc-packages = {
          inherit (self.packages.${system}) fc-common fc-evaluator fc-migrate-cli fc-queue-runner fc-server;
        };
      };

      # FC Packages
      fc-common = pkgs.callPackage ./nix/packages/fc-common.nix {
        inherit craneLib commonArgs cargoArtifacts;
      };

      fc-evaluator = pkgs.callPackage ./nix/packages/fc-evaluator.nix {
        inherit craneLib commonArgs cargoArtifacts;
      };

      fc-migrate-cli = pkgs.callPackage ./nix/packages/fc-migrate-cli.nix {
        inherit craneLib commonArgs cargoArtifacts;
      };

      fc-queue-runner = pkgs.callPackage ./nix/packages/fc-queue-runner.nix {
        inherit craneLib commonArgs cargoArtifacts;
      };

      fc-server = pkgs.callPackage ./nix/packages/fc-server.nix {
        inherit craneLib commonArgs cargoArtifacts;
      };
    });

    checks = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      vm-test = pkgs.callPackage ./nix/vm-test.nix {
        nixosModule = self.nixosModules.default;
        fc-packages = {
          inherit (self.packages.${system}) fc-common fc-evaluator fc-migrate-cli fc-queue-runner fc-server;
        };
      };
    });

    devShells = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      craneLib = crane.mkLib pkgs;
    in {
      default = craneLib.devShell {
        name = "fc";
        inputsFrom = [self.packages.${system}.fc-server];

        strictDeps = true;
        packages = with pkgs; [
          rust-analyzer
          postgresql
          pkg-config
          openssl
        ];
      };
    });
  };
}

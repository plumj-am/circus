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
      testArgs = {
        nixosModule = self.nixosModules.default;
        fc-packages = {
          inherit (self.packages.${system}) fc-common fc-evaluator fc-migrate-cli fc-queue-runner fc-server;
        };
      };
    in {
      # Split VM integration tests
      service-startup = pkgs.callPackage ./nix/tests/service-startup.nix testArgs;
      basic-api = pkgs.callPackage ./nix/tests/basic-api.nix testArgs;
      auth-rbac = pkgs.callPackage ./nix/tests/auth-rbac.nix testArgs;
      api-crud = pkgs.callPackage ./nix/tests/api-crud.nix testArgs;
      features = pkgs.callPackage ./nix/tests/features.nix testArgs;
      e2e = pkgs.callPackage ./nix/tests/e2e.nix testArgs;

      # Legacy monolithic test (for reference, can be removed after split tests pass)
      vm-test = pkgs.callPackage ./nix/vm-test.nix testArgs;
    });

    devShells = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      default = pkgs.mkShell {
        name = "fc";
        inputsFrom = [self.packages.${system}.fc-server];

        packages = with pkgs; [
          postgresql
          pkg-config
          openssl

          taplo
          (rustfmt.override {asNightly = true;})
        ];
      };
    });
  };
}

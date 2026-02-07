{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs?ref=nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    nixpkgs,
    crane,
    self,
    rust-overlay,
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
      pkgs = nixpkgs.legacyPackages.${system}.extend rust-overlay.overlays.default;
      craneLib = (crane.mkLib pkgs).overrideToolchain (p:
        # Build tools
        # We use the rust-overlay to get the stable Rust toolchain for various targets.
        # This is not exactly necessary, but it allows for compiling for various targets
        # with the least amount of friction.
          p.rust-bin.nightly.latest.default.override {
            extensions = ["rustfmt" "rust-analyzer" "clippy"];
            targets = [];
          });

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
      demo-vm = pkgs.callPackage ./nix/demo-vm.nix {inherit self;};

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
      # Split VM integration tests
      service-startup = pkgs.callPackage ./nix/tests/startup.nix {inherit self;};
      basic-api = pkgs.callPackage ./nix/tests/basic-api.nix {inherit self;};
      auth-rbac = pkgs.callPackage ./nix/tests/auth-rbac.nix {inherit self;};
      api-crud = pkgs.callPackage ./nix/tests/api-crud.nix {inherit self;};
      features = pkgs.callPackage ./nix/tests/features.nix {inherit self;};
      webhooks = pkgs.callPackage ./nix/tests/webhooks.nix {inherit self;};
      e2e = pkgs.callPackage ./nix/tests/e2e.nix {inherit self;};
    });

    devShells = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      default = pkgs.mkShell {
        name = "fc-dev";
        inputsFrom = [self.packages.${system}.fc-server];

        strictDeps = true;
        packages = with pkgs; [
          pkg-config
          openssl
          postgresql_18

          taplo
          cargo-nextest
        ];
      };
    });

    formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.alejandra);
  };
}

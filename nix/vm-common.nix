{
  self,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.modules) mkDefault;
  fc-packages = self.packages.${pkgs.stdenv.hostPlatform.system};
in {
  # Common machine configuration for all FC integration tests
  config = {
    ## VM hardware
    virtualisation = {
      memorySize = 2048;
      cores = 2;
      diskSize = 10000;
      graphics = false;

      # Forward guest:3000 -> host:3000 so the dashboard is reachable
      forwardPorts = [
        {
          from = "host";
          host.port = 3000;
          guest.port = 3000;
        }
      ];
    };

    # Machine config
    programs.git.enable = true;
    security.sudo.enable = true;

    # Ensure nix and zstd are available for cache endpoints
    environment.systemPackages = with pkgs; [nix nix-eval-jobs zstd curl jq openssl];

    # Enable Nix flakes and nix-command experimental features required by evaluator
    nix.settings.experimental-features = ["nix-command" "flakes"];

    # VM tests have no network. We need to disable substituters to prevent
    # Nix from trying to contact cache.nixos.org and timing out each time.
    nix.settings.substituters = lib.mkForce [];

    # Allow incoming requests on port 3000 to make the dashboard accessible from
    # the host machine.
    networking.firewall.allowedTCPPorts = [3000];

    services.fc-ci = {
      enable = true;

      package = mkDefault fc-packages.fc-server;
      evaluatorPackage = mkDefault fc-packages.fc-evaluator;
      queueRunnerPackage = mkDefault fc-packages.fc-queue-runner;
      migratePackage = mkDefault fc-packages.fc-migrate-cli;

      server.enable = true;
      evaluator.enable = true;
      queueRunner.enable = true;

      settings = {
        database.url = "postgresql:///fc?host=/run/postgresql";
        server = {
          host = "127.0.0.1";
          port = 3000;
          cors_permissive = false;
        };

        gc.enabled = false;
        logs.log_dir = "/var/lib/fc/logs";
        cache.enabled = true;
        signing.enabled = false;

        tracing = {
          level = "info";
          format = "compact";
          show_targets = true;
          show_timestamps = true;
        };

        evaluator = {
          poll_interval = 5;
          work_dir = "/var/lib/fc/evaluator";
          nix_timeout = 60;
        };

        queue_runner = {
          poll_interval = 3;
          work_dir = "/var/lib/fc/queue-runner";
        };
      };

      # Declarative configuration for VM tests
      # This is set outside of settings so the NixOS module can transform field names
      declarative.apiKeys = [
        {
          name = "bootstrap-admin";
          key = "fc_bootstrap_key";
          role = "admin";
        }
      ];

      # Declarative project for tests that expect bootstrapped data
      # Jobset is disabled so evaluator won't try to fetch from GitHub
      declarative.projects = [
        {
          name = "declarative-project";
          repositoryUrl = "https://github.com/test/declarative";
          description = "Test declarative project";
          jobsets = [
            {
              name = "packages";
              nixExpression = "packages";
              flakeMode = true;
              enabled = true;
              checkInterval = 3600;
              state = "disabled"; # disabled: exists but won't be evaluated
            }
          ];
        }
      ];
    };
  };
}

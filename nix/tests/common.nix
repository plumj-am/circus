# Common machine configuration for all FC integration tests
{
  pkgs,
  fc-packages,
  nixosModule,
}: {
  imports = [nixosModule];

  programs.git.enable = true;
  security.sudo.enable = true;

  # Ensure nix and zstd are available for cache endpoints
  environment.systemPackages = with pkgs; [nix nix-eval-jobs zstd curl jq openssl];

  services.fc = {
    enable = true;
    package = fc-packages.fc-server;
    evaluatorPackage = fc-packages.fc-evaluator;
    queueRunnerPackage = fc-packages.fc-queue-runner;
    migratePackage = fc-packages.fc-migrate-cli;

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

      evaluator.poll_interval = 5;
      evaluator.work_dir = "/var/lib/fc/evaluator";
      queue_runner.poll_interval = 3;
      queue_runner.work_dir = "/var/lib/fc/queue-runner";

      # Declarative bootstrap: project + API key created on startup
      declarative = {
        projects = [
          {
            name = "declarative-project";
            repository_url = "https://github.com/test/declarative";
            description = "Bootstrap test project";
            jobsets = [
              {
                name = "packages";
                nix_expression = "packages";
                enabled = true;
                flake_mode = true;
                check_interval = 300;
              }
            ];
          }
        ];

        api_keys = [
          {
            name = "bootstrap-admin";
            key = "fc_bootstrap_key";
            role = "admin";
          }
        ];
      };
    };
  };
}

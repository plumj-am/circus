{
  config,
  pkgs,
  lib,
  ...
}: let
  inherit (lib.options) mkOption mkEnableOption;
  inherit (lib.types) bool str int package listOf submodule nullOr;
  cfg = config.services.fc;

  settingsFormat = pkgs.formats.toml {};
  settingsType = settingsFormat.type;

  # Build the final settings by merging declarative config into settings
  finalSettings = lib.recursiveUpdate cfg.settings (lib.optionalAttrs (cfg.declarative.projects != [] || cfg.declarative.apiKeys != []) {
    declarative = {
      projects = map (p: {
        name = p.name;
        repository_url = p.repositoryUrl;
        description = p.description or null;
        jobsets = map (j: {
          name = j.name;
          nix_expression = j.nixExpression;
          enabled = j.enabled;
          flake_mode = j.flakeMode;
          check_interval = j.checkInterval;
        }) p.jobsets;
      }) cfg.declarative.projects;
      api_keys = map (k: {
        name = k.name;
        key = k.key;
        role = k.role;
      }) cfg.declarative.apiKeys;
    };
  });

  settingsFile = settingsFormat.generate "fc.toml" finalSettings;

  inherit (builtins) map;

  jobsetOpts = {
    options = {
      name = mkOption {
        type = str;
        description = "Jobset name.";
      };
      nixExpression = mkOption {
        type = str;
        description = "Nix expression to evaluate (e.g. 'packages', 'checks', 'hydraJobs').";
      };
      enabled = mkOption {
        type = bool;
        default = true;
        description = "Whether this jobset is enabled for evaluation.";
      };
      flakeMode = mkOption {
        type = bool;
        default = true;
        description = "Whether to evaluate as a flake.";
      };
      checkInterval = mkOption {
        type = int;
        default = 60;
        description = "Seconds between evaluation checks.";
      };
    };
  };

  projectOpts = {
    options = {
      name = mkOption {
        type = str;
        description = "Project name (unique identifier).";
      };
      repositoryUrl = mkOption {
        type = str;
        description = "Git repository URL.";
      };
      description = mkOption {
        type = nullOr str;
        default = null;
        description = "Optional project description.";
      };
      jobsets = mkOption {
        type = listOf (submodule jobsetOpts);
        default = [];
        description = "Jobsets to create for this project.";
      };
    };
  };

  apiKeyOpts = {
    options = {
      name = mkOption {
        type = str;
        description = "Human-readable name for this API key.";
      };
      key = mkOption {
        type = str;
        description = ''
          The raw API key value (e.g. "fc_mykey123").
          Will be hashed before storage. Consider using a secrets manager.
        '';
      };
      role = mkOption {
        type = str;
        default = "admin";
        description = "Role: admin, read-only, create-projects, eval-jobset, cancel-build, restart-jobs, bump-to-front.";
      };
    };
  };
in {
  options.services.fc = {
    enable = mkEnableOption "FC CI system";

    package = mkOption {
      type = package;
      description = "The FC server package.";
    };

    evaluatorPackage = mkOption {
      type = package;
      default = cfg.package;
      description = "The FC evaluator package. Defaults to cfg.package.";
    };

    queueRunnerPackage = mkOption {
      type = package;
      default = cfg.package;
      description = "The FC queue runner package. Defaults to cfg.package.";
    };

    migratePackage = mkOption {
      type = package;
      description = "The FC migration CLI package.";
    };

    settings = mkOption {
      type = settingsType;
      default = {};
      description = ''
        FC configuration as a Nix attribute set.
        Will be converted to TOML and written to fc.toml.
      '';
    };

    declarative = {
      projects = mkOption {
        type = listOf (submodule projectOpts);
        default = [];
        description = ''
          Declarative project definitions. These are upserted on every
          server startup, ensuring the database matches this configuration.
        '';
        example = lib.literalExpression ''
          [
            {
              name = "my-project";
              repositoryUrl = "https://github.com/user/repo";
              description = "My Nix project";
              jobsets = [
                { name = "packages"; nixExpression = "packages"; }
                { name = "checks"; nixExpression = "checks"; }
              ];
            }
          ]
        '';
      };

      apiKeys = mkOption {
        type = listOf (submodule apiKeyOpts);
        default = [];
        description = ''
          Declarative API key definitions. Keys are upserted on every
          server startup. Use a secrets manager for production deployments.
        '';
        example = lib.literalExpression ''
          [
            { name = "admin"; key = "fc_admin_secret"; role = "admin"; }
            { name = "ci-bot"; key = "fc_ci_bot_key"; role = "eval-jobset"; }
          ]
        '';
      };
    };

    database = {
      createLocally = mkOption {
        type = bool;
        default = true;
        description = "Whether to create the PostgreSQL database locally.";
      };
    };

    server = {
      enable = mkEnableOption "FC server (REST API)";
    };

    evaluator = {
      enable = mkEnableOption "FC evaluator (git polling and nix evaluation)";
    };

    queueRunner = {
      enable = mkEnableOption "FC queue runner (build dispatch)";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.fc = {
      isSystemUser = true;
      group = "fc";
      home = "/var/lib/fc";
      createHome = true;
    };

    users.groups.fc = {};

    services.postgresql = lib.mkIf cfg.database.createLocally {
      enable = true;
      ensureDatabases = ["fc"];
      ensureUsers = [
        {
          name = "fc";
          ensureDBOwnership = true;
        }
      ];
    };

    services.fc.settings = lib.mkDefault {
      database.url = "postgresql:///fc?host=/run/postgresql";
      server.host = "127.0.0.1";
      server.port = 3000;
      gc.gc_roots_dir = "/nix/var/nix/gcroots/per-user/fc/fc-roots";
      gc.enabled = true;
      gc.max_age_days = 30;
      gc.cleanup_interval = 3600;
      logs.log_dir = "/var/lib/fc/logs";
      cache.enabled = true;
      evaluator.restrict_eval = true;
      evaluator.allow_ifd = false;
      signing.enabled = false;
    };

    systemd.tmpfiles.rules = [
      (lib.mkIf cfg.server.enable "d /var/lib/fc/logs 0750 fc fc -")
      (lib.mkIf cfg.queueRunner.enable "d /nix/var/nix/gcroots/per-user/fc 0755 fc fc -")
    ];

    systemd.services.fc-server = lib.mkIf cfg.server.enable {
      description = "FC CI Server";
      wantedBy = ["multi-user.target"];
      after = ["network.target"] ++ lib.optional cfg.database.createLocally "postgresql.target";
      requires = lib.optional cfg.database.createLocally "postgresql.target";

      path = with pkgs; [nix zstd];

      serviceConfig = {
        ExecStartPre = "${cfg.migratePackage}/bin/fc-migrate up ${finalSettings.database.url or "postgresql:///fc?host=/run/postgresql"}";
        ExecStart = "${cfg.package}/bin/fc-server";
        Restart = "on-failure";
        RestartSec = 5;
        User = "fc";
        Group = "fc";
        StateDirectory = "fc";
        LogsDirectory = "fc";
        WorkingDirectory = "/var/lib/fc";
        ReadWritePaths = ["/var/lib/fc"];

        # Hardening
        ProtectSystem = "strict";
        ProtectHome = true;
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
      };

      environment = {
        FC_CONFIG_FILE = "${settingsFile}";
      };
    };

    systemd.services.fc-evaluator = lib.mkIf cfg.evaluator.enable {
      description = "FC CI Evaluator";
      wantedBy = ["multi-user.target"];
      after = ["network.target" "fc-server.service"] ++ lib.optional cfg.database.createLocally "postgresql.target";
      requires = ["fc-server.service"] ++ lib.optional cfg.database.createLocally "postgresql.target";

      path = with pkgs; [
        nix
        git
        nix-eval-jobs
      ];

      serviceConfig = {
        ExecStart = "${cfg.evaluatorPackage}/bin/fc-evaluator";
        Restart = "on-failure";
        RestartSec = 10;
        User = "fc";
        Group = "fc";
        StateDirectory = "fc";
        WorkingDirectory = "/var/lib/fc";
        ReadWritePaths = ["/var/lib/fc"];

        # Hardening
        ProtectSystem = "strict";
        ProtectHome = true;
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
      };

      environment = {
        FC_CONFIG_FILE = "${settingsFile}";
        FC_EVALUATOR__WORK_DIR = "/var/lib/fc/evaluator";
        FC_EVALUATOR__RESTRICT_EVAL = "true";
      };
    };

    systemd.services.fc-queue-runner = lib.mkIf cfg.queueRunner.enable {
      description = "FC CI Queue Runner";
      wantedBy = ["multi-user.target"];
      after = ["network.target" "fc-server.service"] ++ lib.optional cfg.database.createLocally "postgresql.target";
      requires = ["fc-server.service"] ++ lib.optional cfg.database.createLocally "postgresql.target";

      path = with pkgs; [
        nix
      ];

      serviceConfig = {
        ExecStart = "${cfg.queueRunnerPackage}/bin/fc-queue-runner";
        Restart = "on-failure";
        RestartSec = 10;
        User = "fc";
        Group = "fc";
        StateDirectory = "fc";
        LogsDirectory = "fc";
        WorkingDirectory = "/var/lib/fc";
        ReadWritePaths = [
          "/var/lib/fc"
          "/nix/var/nix/gcroots/per-user/fc"
        ];

        # Hardening
        ProtectSystem = "strict";
        ProtectHome = true;
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
      };

      environment = {
        FC_CONFIG_FILE = "${settingsFile}";
        FC_QUEUE_RUNNER__WORK_DIR = "/var/lib/fc/queue-runner";
      };
    };
  };
}

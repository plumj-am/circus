{
  pkgs,
  self,
}: let
  fc-packages = self.packages.${pkgs.stdenv.hostPlatform.system};
  nixos = pkgs.nixos ({
    modulesPath,
    pkgs,
    ...
  }: {
    imports = [
      self.nixosModules.fc-ci
      (modulesPath + "/virtualisation/qemu-vm.nix")
    ];

    ## VM hardware
    virtualisation = {
      memorySize = 2048;
      cores = 2;
      diskSize = 4096;
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
        gc.enabled = false;
        logs.log_dir = "/var/lib/fc/logs";
        cache.enabled = true;
        signing.enabled = false;
        server = {
          # Bind to all interfaces so port forwarding works
          host = "0.0.0.0";
          port = 3000;
          cors_permissive = true;
        };
      };
    };

    ## Seed an admin API key on first boot
    # Token: fc_demo_admin_key, SHA-256 hash inserted into api_keys
    # A read-only key is also seeded for testing RBAC.
    systemd.services.fc-seed-keys = {
      description = "Seed demo API keys";
      after = ["fc-server.service"];
      requires = ["fc-server.service"];
      wantedBy = ["multi-user.target"];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        User = "fc";
        Group = "fc";
      };
      path = [pkgs.postgresql pkgs.curl];
      script = ''
        # Wait for server to be ready
        for i in $(seq 1 30); do
          if curl -sf http://127.0.0.1:3000/health >/dev/null 2>&1; then
            break
          fi
          sleep 1
        done

        # Admin key: fc_demo_admin_key
        ADMIN_HASH="$(echo -n 'fc_demo_admin_key' | sha256sum | cut -d' ' -f1)"
        psql -U fc -d fc -c "INSERT INTO api_keys (name, key_hash, role) VALUES ('demo-admin', '$ADMIN_HASH', 'admin') ON CONFLICT DO NOTHING" 2>/dev/null || true

        # Read-only key: fc_demo_readonly_key
        RO_HASH="$(echo -n 'fc_demo_readonly_key' | sha256sum | cut -d' ' -f1)"
        psql -U fc -d fc -c "INSERT INTO api_keys (name, key_hash, role) VALUES ('demo-readonly', '$RO_HASH', 'read-only') ON CONFLICT DO NOTHING" 2>/dev/null || true

        echo ""
        echo "==========================================="
        echo ""
        echo "  Dashboard:  http://localhost:3000"
        echo "  Health:     http://localhost:3000/health"
        echo "  API base:   http://localhost:3000/api/v1"
        echo ""
        echo "  Admin key:     fc_demo_admin_key"
        echo "  Read-only key: fc_demo_readonly_key"
        echo ""
        echo "  Login at http://localhost:3000/login"
        echo "  using the admin key above."
        echo "==========================================="
      '';
    };

    # --- Useful tools inside the VM ---
    environment.systemPackages = with pkgs; [
      curl
      jq
      htop
      nix
      nix-eval-jobs
      git
      zstd
    ];

    # --- Misc VM settings ---
    networking.hostName = "fc-demo";
    networking.firewall.allowedTCPPorts = [3000];
    services.getty.autologinUser = "root";

    # Show a helpful MOTD
    environment.etc."motd".text = ''
      ┌──────────────────────────────────────────────┐
      │  Dashboard:  http://localhost:3000           │
      │  API:        http://localhost:3000/api/v1    │
      │                                              │
      │  Admin API key:     fc_demo_admin_key        │
      │  Read-only API key: fc_demo_readonly_key     │
      │                                              │
      │  Useful commands:                            │
      │    $ systemctl status fc-server              │
      │    $ journalctl -u fc-server -f              │
      │    $ curl -sf localhost:3000/health | jq     │
      │    $ curl -sf localhost:3000/metrics         │
      │                                              │
      │  Press Ctrl-a x to quit QEMU.                │
      └──────────────────────────────────────────────┘
    '';

    system.stateVersion = "26.11";
  });
in
  pkgs.writeShellApplication {
    name = "run-fc-demo-vm";
    text = ''
      echo "Starting FC CI demo VM..."
      echo "Dashboard will be available at http://localhost:3000"
      echo "Press Ctrl-a x to quit."
      echo ""
      exec ${nixos.config.system.build.vm}/bin/run-fc-demo-vm
    '';
  }

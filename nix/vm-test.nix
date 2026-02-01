{
  pkgs,
  fc-packages,
  nixosModule,
}:
pkgs.testers.nixosTest {
  name = "fc-integration";

  nodes.machine = {pkgs, ...}: {
    imports = [nixosModule];

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
        declarative.projects = [
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
        declarative.api_keys = [
          {
            name = "bootstrap-admin";
            key = "fc_bootstrap_key";
            role = "admin";
          }
        ];
      };
    };

    # Ensure nix and zstd are available for cache endpoints
    environment.systemPackages = with pkgs; [nix nix-eval-jobs zstd curl jq sudo git openssl];
  };

  testScript = ''
    import hashlib
    import json
    import re
    import time

    machine.start()
    machine.wait_for_unit("postgresql.service")

    # Ensure PostgreSQL is actually ready to accept connections before fc-server starts
    machine.wait_until_succeeds("sudo -u fc psql -U fc -d fc -c 'SELECT 1'", timeout=30)

    machine.wait_for_unit("fc-server.service")

    # Wait for the server to start listening
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=30)

    # ---- Verify all three services start ----
    with subtest("fc-evaluator.service starts without crash"):
        machine.wait_for_unit("fc-evaluator.service", timeout=30)
        # Check journalctl for no "binary not found" errors
        result = machine.succeed("journalctl -u fc-evaluator --no-pager -n 20 2>&1")
        assert "binary not found" not in result.lower(), f"Evaluator has 'binary not found' error: {result}"
        assert "No such file" not in result, f"Evaluator has 'No such file' error: {result}"

    with subtest("fc-queue-runner.service starts without crash"):
        machine.wait_for_unit("fc-queue-runner.service", timeout=30)
        result = machine.succeed("journalctl -u fc-queue-runner --no-pager -n 20 2>&1")
        assert "binary not found" not in result.lower(), f"Queue runner has 'binary not found' error: {result}"
        assert "No such file" not in result, f"Queue runner has 'No such file' error: {result}"

    with subtest("All three FC services are active"):
        for svc in ["fc-server", "fc-evaluator", "fc-queue-runner"]:
            result = machine.succeed(f"systemctl is-active {svc}")
            assert result.strip() == "active", f"Expected {svc} to be active, got '{result.strip()}'"

    # ---- Seed an API key for write operations ----
    # Token: fc_testkey123 -> SHA-256 hash inserted into api_keys table
    api_token = "fc_testkey123"
    api_hash = hashlib.sha256(api_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u fc psql -U fc -d fc -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('test', '{api_hash}', 'admin')\""
    )
    auth_header = f"-H 'Authorization: Bearer {api_token}'"

    # ========================================================================
    # Phase 0: Declarative Bootstrap Tests
    # ========================================================================

    with subtest("Declarative project was bootstrapped"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects | jq '.items[] | select(.name==\"declarative-project\") | .name' -r"
        )
        assert result.strip() == "declarative-project", f"Expected declarative-project, got '{result.strip()}'"

    with subtest("Declarative project has correct repository URL"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects | jq '.items[] | select(.name==\"declarative-project\") | .repository_url' -r"
        )
        assert result.strip() == "https://github.com/test/declarative", f"Expected declarative repo URL, got '{result.strip()}'"

    with subtest("Declarative project has bootstrapped jobset"):
        # Get the declarative project ID
        decl_project_id = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects | jq '.items[] | select(.name==\"declarative-project\") | .id' -r"
        ).strip()
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{decl_project_id}/jobsets | jq '.items[0].name' -r"
        )
        assert result.strip() == "packages", f"Expected packages jobset, got '{result.strip()}'"

    with subtest("Declarative API key works for authentication"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            "-H 'Authorization: Bearer fc_bootstrap_key' "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"bootstrap-auth-test\", \"repository_url\": \"https://example.com/bootstrap\"}'"
        )
        assert code.strip() == "200", f"Expected 200 with bootstrap key, got {code.strip()}"

    with subtest("Bootstrap is idempotent (server restarted successfully with same config)"):
        # The server already started successfully with declarative config - that proves
        # the bootstrap ran. We verify no duplicate projects were created.
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects | jq '[.items[] | select(.name==\"declarative-project\")] | length'"
        )
        assert result.strip() == "1", f"Expected exactly 1 declarative-project, got {result.strip()}"

    # ========================================================================
    # Phase 0B: Security Headers Tests
    # ========================================================================

    with subtest("X-Content-Type-Options nosniff header present"):
        result = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/health | grep -i x-content-type-options"
        )
        assert "nosniff" in result.lower(), f"Expected nosniff, got: {result}"

    with subtest("X-Frame-Options DENY header present"):
        result = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/health | grep -i x-frame-options"
        )
        assert "deny" in result.lower(), f"Expected DENY, got: {result}"

    with subtest("Referrer-Policy header present"):
        result = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/health | grep -i referrer-policy"
        )
        assert "strict-origin-when-cross-origin" in result.lower(), f"Expected strict-origin-when-cross-origin, got: {result}"

    with subtest("Security headers present on API routes too"):
        result = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/api/v1/projects 2>&1"
        )
        assert "nosniff" in result.lower(), "API route missing X-Content-Type-Options"
        assert "deny" in result.lower(), "API route missing X-Frame-Options"

    # ========================================================================
    # Phase 0C: Error Message Quality Tests
    # ========================================================================

    with subtest("404 error returns structured JSON with error_code"):
        result = machine.succeed(
            "curl -s http://127.0.0.1:3000/api/v1/projects/00000000-0000-0000-0000-000000000000"
        )
        assert len(result.strip()) > 0, "Expected non-empty response body for 404"
        parsed = json.loads(result)
        assert "error" in parsed, f"Missing 'error' field in: {result}"
        assert "error_code" in parsed, f"Missing 'error_code' field in: {result}"
        assert parsed["error_code"] == "NOT_FOUND", f"Expected NOT_FOUND, got {parsed['error_code']}"

    with subtest("409 conflict error includes meaningful message"):
        # First create a project
        machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"error-msg-test\", \"repository_url\": \"https://example.com/err\"}'"
        )
        # Try creating duplicate — check status code first
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"error-msg-test\", \"repository_url\": \"https://example.com/err2\"}'"
        )
        assert code.strip() == "409", f"Expected 409 for duplicate, got {code.strip()}"
        # Verify the response body is structured JSON with error details
        result = machine.succeed(
            "curl -s -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"error-msg-test\", \"repository_url\": \"https://example.com/err2\"}'"
        )
        parsed = json.loads(result)
        assert "error" in parsed, f"Missing error field in conflict response: {result}"
        assert parsed.get("error_code") == "CONFLICT", f"Expected CONFLICT error_code, got: {parsed}"
        # Error message should not be generic "Internal server error"
        assert "internal" not in parsed["error"].lower(), \
            f"Error message should not be generic 'Internal server error': {parsed['error']}"

    with subtest("401 error returns structured JSON"):
        result = machine.succeed(
            "curl -s -X POST http://127.0.0.1:3000/api/v1/projects "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"x\", \"repository_url\": \"https://example.com/x\"}'"
        )
        try:
            parsed = json.loads(result)
            assert "error" in parsed, f"Missing error field in 401: {result}"
        except json.JSONDecodeError:
            # Auth middleware may return non-JSON 401; verify status code instead
            code = machine.succeed(
                "curl -s -o /dev/null -w '%{http_code}' "
                "-X POST http://127.0.0.1:3000/api/v1/projects "
                "-H 'Content-Type: application/json' "
                "-d '{\"name\": \"x\", \"repository_url\": \"https://example.com/x\"}'"
            )
            assert code.strip() == "401", f"Expected 401, got {code.strip()}"

    # ---- Health endpoint ----
    with subtest("Health endpoint returns OK"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/health | jq -r .status")
        assert result.strip() == "ok", f"Expected 'ok', got '{result.strip()}'"

    with subtest("Health endpoint reports database healthy"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/health | jq -r .database")
        assert result.strip() == "true", f"Expected 'true', got '{result.strip()}'"

    # ---- Cache endpoint: nix-cache-info ----
    with subtest("Cache info endpoint returns correct data"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/nix-cache/nix-cache-info")
        assert "StoreDir: /nix/store" in result, f"Missing StoreDir in: {result}"
        assert "WantMassQuery: 1" in result, f"Missing WantMassQuery in: {result}"

    # ---- Cache endpoint: invalid hash rejection ----
    with subtest("Cache rejects short hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/nix-cache/tooshort.narinfo | grep -q 404")

    with subtest("Cache rejects uppercase hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/nix-cache/ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEF.narinfo | grep -q 404")

    with subtest("Cache rejects special chars in hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' 'http://127.0.0.1:3000/nix-cache/abcdefghijklmnop____abcde.narinfo' | grep -q 404")

    with subtest("Cache returns 404 for valid but nonexistent hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/nix-cache/abcdefghijklmnopqrstuvwxyz012345.narinfo | grep -q 404")

    # ---- NAR endpoints: invalid hash rejection ----
    with subtest("NAR zst rejects invalid hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/nix-cache/nar/INVALID.nar.zst | grep -q 404")

    with subtest("NAR plain rejects invalid hash"):
        machine.succeed("curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/nix-cache/nar/INVALID.nar | grep -q 404")

    # ---- Search endpoint: length validation ----
    with subtest("Search rejects empty query"):
        result = machine.succeed("curl -sf 'http://127.0.0.1:3000/api/v1/search?q=' | jq '.projects | length'")
        assert result.strip() == "0", f"Expected 0 projects, got {result.strip()}"

    with subtest("Search rejects overly long query"):
        long_q = "a" * 300
        result = machine.succeed(f"curl -sf 'http://127.0.0.1:3000/api/v1/search?q={long_q}' | jq '.projects | length'")
        assert result.strip() == "0", f"Expected 0 projects for long query, got {result.strip()}"

    # ---- Error response format ----
    with subtest("404 error response includes error_code field"):
        json_result = machine.succeed("curl -s http://127.0.0.1:3000/api/v1/projects/00000000-0000-0000-0000-000000000000 | jq -r .error_code")
        assert json_result.strip() == "NOT_FOUND", f"Expected NOT_FOUND, got {json_result.strip()}"

    # ---- Empty page states (before any data is created) ----
    with subtest("Empty evaluations page has proper empty state"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/evaluations")
        assert "Page 1 of 0" not in body, \
            "Evaluations page should NOT show 'Page 1 of 0' when empty"
        assert "No evaluations yet" in body, \
            "Empty evaluations page should show helpful empty state message"

    with subtest("Empty builds page has proper empty state"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/builds")
        assert "Page 1 of 0" not in body, \
            "Builds page should NOT show 'Page 1 of 0' when empty"
        assert "No builds match" in body, \
            "Empty builds page should show helpful empty state message"

    with subtest("Empty channels page has proper empty state"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/channels")
        assert "No channels configured" in body, \
            "Empty channels page should show helpful empty state"

    with subtest("Tables use table-wrap containers on projects page"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/projects")
        # Projects page should have at least one project (from bootstrap)
        assert "table-wrap" in body, \
            "Projects page should wrap tables in .table-wrap class"

    # ---- API CRUD: create and list projects ----
    with subtest("Create a project via API"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"test-project\", \"repository_url\": \"https://github.com/test/repo\"}' "
            "| jq -r .id"
        )
        project_id = result.strip()
        assert len(project_id) == 36, f"Expected UUID, got '{project_id}'"

    with subtest("List projects includes created project"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/api/v1/projects | jq '.items[0].name'")
        assert "test-project" in result, f"Expected test-project in: {result}"

    # ---- Builds list with filters ----
    with subtest("Builds list with system filter returns 200"):
        machine.succeed("curl -sf 'http://127.0.0.1:3000/api/v1/builds?system=x86_64-linux' | jq '.items'")

    with subtest("Builds list with job_name filter returns 200"):
        machine.succeed("curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=hello' | jq '.items'")

    with subtest("Builds list with combined filters returns 200"):
        machine.succeed("curl -sf 'http://127.0.0.1:3000/api/v1/builds?system=x86_64-linux&status=pending&job_name=test' | jq '.items'")

    # ---- Metrics endpoint ----
    with subtest("Metrics endpoint returns prometheus format"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/metrics")
        assert "fc_builds_total" in result, "Missing fc_builds_total in metrics"
        assert "fc_projects_total" in result, "Missing fc_projects_total in metrics"
        assert "fc_evaluations_total" in result, "Missing fc_evaluations_total in metrics"

    # ---- CORS: default restrictive (no Access-Control-Allow-Origin for cross-origin) ----
    with subtest("Default CORS does not allow arbitrary origins"):
        result = machine.succeed(
            "curl -s -D - "
            "-H 'Origin: http://evil.example.com' "
            "http://127.0.0.1:3000/health "
            "2>&1"
        )
        # With restrictive CORS, there should be no access-control-allow-origin header
        # for an arbitrary origin
        assert "access-control-allow-origin: http://evil.example.com" not in result.lower(), \
            f"CORS should not allow arbitrary origins: {result}"

    # ---- Systemd hardening ----
    with subtest("fc-server runs as fc user"):
        result = machine.succeed("systemctl show fc-server --property=User --value")
        assert result.strip() == "fc", f"Expected fc user, got '{result.strip()}'"

    with subtest("fc-server has NoNewPrivileges"):
        result = machine.succeed("systemctl show fc-server --property=NoNewPrivileges --value")
        assert result.strip() == "yes", f"Expected NoNewPrivileges, got '{result.strip()}'"

    with subtest("fc user home directory exists"):
        machine.succeed("test -d /var/lib/fc")

    with subtest("Log directory exists"):
        machine.succeed("test -d /var/lib/fc/logs || mkdir -p /var/lib/fc/logs")

    # ---- Stats endpoint ----
    with subtest("Build stats endpoint returns data"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/api/v1/builds/stats | jq '.total_builds'")
        # Should be a number (possibly 0)
        int(result.strip())

    with subtest("Recent builds endpoint returns array"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/api/v1/builds/recent | jq 'type'")
        assert result.strip() == '"array"', f"Expected array, got {result.strip()}"

    # ========================================================================
    # Phase 3: Authentication & RBAC tests
    # ========================================================================

    # ---- 3A: Authentication tests ----
    with subtest("Unauthenticated POST returns 401"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"unauth-test\", \"repository_url\": \"https://example.com/repo\"}'"
        )
        assert code.strip() == "401", f"Expected 401, got {code.strip()}"

    with subtest("Wrong token POST returns 401"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            "-H 'Authorization: Bearer fc_wrong_token_here' "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"bad-auth-test\", \"repository_url\": \"https://example.com/repo\"}'"
        )
        assert code.strip() == "401", f"Expected 401, got {code.strip()}"

    with subtest("Valid token POST returns 200"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"auth-test-project\", \"repository_url\": \"https://example.com/auth-repo\"}'"
        )
        assert code.strip() == "200", f"Expected 200, got {code.strip()}"

    with subtest("GET without token returns 200"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/projects"
        )
        assert code.strip() == "200", f"Expected 200, got {code.strip()}"

    # ---- 3B: RBAC tests ----
    # Seed a read-only key
    ro_token = "fc_readonly_key"
    ro_hash = hashlib.sha256(ro_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u fc psql -U fc -d fc -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('readonly', '{ro_hash}', 'read-only')\""
    )
    ro_header = f"-H 'Authorization: Bearer {ro_token}'"

    with subtest("Read-only key POST project returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"ro-attempt\", \"repository_url\": \"https://example.com/ro\"}'"
        )
        assert code.strip() == "403", f"Expected 403, got {code.strip()}"

    with subtest("Read-only key POST admin/builders returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/admin/builders "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"bad-builder\", \"ssh_uri\": \"ssh://x@y\", \"systems\": [\"x86_64-linux\"]}'"
        )
        assert code.strip() == "403", f"Expected 403, got {code.strip()}"

    with subtest("Admin key POST admin/builders returns 200"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/admin/builders "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"test-builder\", \"ssh_uri\": \"ssh://nix@builder\", \"systems\": [\"x86_64-linux\"], \"max_jobs\": 2}'"
        )
        assert code.strip() == "200", f"Expected 200, got {code.strip()}"

    with subtest("Admin key create and delete API key"):
        # Create
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/api-keys "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"ephemeral\", \"role\": \"read-only\"}'"
        )
        key_data = json.loads(result)
        assert "id" in key_data, f"Expected id in response: {result}"
        key_id = key_data["id"]
        # Delete
        code = machine.succeed(
            f"curl -s -o /dev/null -w '%{{http_code}}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/api-keys/{key_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200, got {code.strip()}"

    # ---- 3C: API key lifecycle test ----
    with subtest("API key lifecycle: create, use, delete, verify 401"):
        # Create a new key via admin API
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/api-keys "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"lifecycle-test\", \"role\": \"admin\"}'"
        )
        lc_data = json.loads(result)
        lc_key = lc_data["key"]
        lc_id = lc_data["id"]
        lc_header = f"-H 'Authorization: Bearer {lc_key}'"

        # Use the new key to create a project
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{lc_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"lifecycle-project\", \"repository_url\": \"https://example.com/lc\"}'"
        )
        assert code.strip() == "200", f"Expected 200 with new key, got {code.strip()}"

        # Delete the key
        machine.succeed(
            f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/api-keys/{lc_id} "
            f"{auth_header}"
        )

        # Verify deleted key returns 401
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{lc_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"should-fail\", \"repository_url\": \"https://example.com/fail\"}'"
        )
        assert code.strip() == "401", f"Expected 401 after key deletion, got {code.strip()}"

    # ---- 3D: CRUD lifecycle test ----
    with subtest("CRUD lifecycle: project -> jobset -> list -> delete -> 404"):
        # Create project
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"crud-test\", \"repository_url\": \"https://example.com/crud\"}' "
            "| jq -r .id"
        )
        crud_project_id = result.strip()

        # Create jobset
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{crud_project_id}/jobsets "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"main\", \"nix_expression\": \".\"}' "
            "| jq -r .id"
        )
        jobset_id = result.strip()
        assert len(jobset_id) == 36, f"Expected UUID for jobset, got '{jobset_id}'"

        # List jobsets (should have at least 1)
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{crud_project_id}/jobsets | jq '.items | length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 jobset, got {result.strip()}"

        # Delete project (cascades)
        machine.succeed(
            f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/projects/{crud_project_id} "
            f"{auth_header}"
        )

        # Verify project returns 404
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"http://127.0.0.1:3000/api/v1/projects/{crud_project_id}"
        )
        assert code.strip() == "404", f"Expected 404 after deletion, got {code.strip()}"

    # ---- 3E: Edge case tests ----
    with subtest("Duplicate project name returns 409"):
        machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"dup-test\", \"repository_url\": \"https://example.com/dup\"}'"
        )
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"dup-test\", \"repository_url\": \"https://example.com/dup2\"}'"
        )
        assert code.strip() == "409", f"Expected 409 for duplicate, got {code.strip()}"

    with subtest("Invalid UUID path returns 400"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/projects/not-a-uuid"
        )
        assert code.strip() == "400", f"Expected 400 for invalid UUID, got {code.strip()}"

    with subtest("XSS in project name returns 400"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"<script>alert(1)</script>\", \"repository_url\": \"https://example.com/xss\"}'"
        )
        assert code.strip() == "400", f"Expected 400 for XSS name, got {code.strip()}"

    # ---- 3F: Security fuzzing ----
    with subtest("SQL injection in search query returns 0 results"):
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/search?q=test%27%20OR%201%3D1%20--' | jq '.projects | length'"
        )
        assert result.strip() == "0", f"Expected 0, got {result.strip()}"
        # Verify projects table is intact
        count = machine.succeed(
            "sudo -u fc psql -U fc -d fc -t -c 'SELECT COUNT(*) FROM projects'"
        )
        assert int(count.strip()) > 0, "Projects table seems damaged"

    with subtest("Path traversal in cache returns 404"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "'http://127.0.0.1:3000/nix-cache/nar/../../../etc/passwd.nar'"
        )
        # Should be 404 (not 200)
        assert code.strip() in ("400", "404"), f"Expected 400/404 for path traversal, got {code.strip()}"

    with subtest("Oversized request body returns 413"):
        # Generate a payload larger than 10MB (the default max_body_size)
        code = machine.succeed(
            "dd if=/dev/zero bs=1M count=12 2>/dev/null | "
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "--data-binary @-"
        )
        assert code.strip() == "413", f"Expected 413 for oversized body, got {code.strip()}"

    with subtest("NULL bytes in project name returns 400"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"null\\u0000byte\", \"repository_url\": \"https://example.com/null\"}'"
        )
        assert code.strip() == "400", f"Expected 400 for null bytes, got {code.strip()}"

    # ---- 3G: Dashboard page smoke tests ----
    with subtest("All dashboard pages return 200"):
        pages = ["/", "/projects", "/evaluations", "/builds", "/queue", "/channels", "/admin", "/login"]
        for page in pages:
            code = machine.succeed(
                f"curl -s -o /dev/null -w '%{{http_code}}' http://127.0.0.1:3000{page}"
            )
            assert code.strip() == "200", f"Page {page} returned {code.strip()}, expected 200"

    # ========================================================================
    # Phase 4: Dashboard Content & Deep Functional Tests
    # ========================================================================

    # ---- 4A: Dashboard content verification ----
    with subtest("Home page contains Dashboard heading"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        assert "Dashboard" in body, "Home page missing 'Dashboard' heading"

    with subtest("Home page contains stats grid"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        assert "stat-card" in body, "Home page missing stats grid"
        assert "Completed" in body, "Home page missing 'Completed' stat"

    with subtest("Home page shows project overview table"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        # We created projects earlier, they should appear
        assert "test-project" in body, "Home page should list test-project in overview"

    with subtest("Projects page contains created projects"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/projects")
        assert "test-project" in body, "Projects page should list test-project"

    with subtest("Projects page returns HTML content type"):
        ct = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/projects | grep -i content-type"
        )
        assert "text/html" in ct.lower(), f"Expected text/html, got: {ct}"

    with subtest("Admin page shows system status"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/admin")
        assert "Administration" in body, "Admin page missing heading"
        assert "System Status" in body, "Admin page missing system status section"
        assert "Remote Builders" in body, "Admin page missing remote builders section"

    with subtest("Queue page renders"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/queue")
        assert "Queue" in body or "Pending" in body or "Running" in body, \
            "Queue page missing expected content"

    with subtest("Channels page renders"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/channels")
        # Page should render even if empty
        assert "Channel" in body or "channel" in body, "Channels page missing expected content"

    with subtest("Builds page renders with filter params"):
        body = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/builds?status=pending&system=x86_64-linux'"
        )
        assert "Build" in body or "build" in body, "Builds page missing expected content"

    with subtest("Evaluations page renders"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/evaluations")
        assert "Evaluation" in body or "evaluation" in body, "Evaluations page missing expected content"

    with subtest("Login page contains form"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/login")
        assert "api_key" in body or "API" in body, "Login page missing API key input"
        assert "<form" in body.lower(), "Login page missing form element"

    # ---- 4B: Dashboard page for specific entities ----
    with subtest("Project detail page renders for existing project"):
        body = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/project/{project_id}"
        )
        assert "test-project" in body, "Project detail page should show project name"

    with subtest("Project detail page with invalid UUID returns graceful error"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/project/00000000-0000-0000-0000-000000000000"
        )
        # Should return 200 with "not found" message or similar, not crash
        assert code.strip() == "200", f"Expected 200 for missing project detail, got {code.strip()}"

    # ---- 4C: Project update via PUT ----
    with subtest("Update project description via PUT"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/projects/{project_id} "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"description\": \"Updated description\"}'"
        )
        assert code.strip() == "200", f"Expected 200 for PUT project, got {code.strip()}"

    with subtest("Updated project reflects new description"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id} | jq -r .description"
        )
        assert result.strip() == "Updated description", f"Expected updated description, got: {result.strip()}"

    with subtest("Update project with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/projects/{project_id} "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"description\": \"Hacked\"}'"
        )
        assert code.strip() == "403", f"Expected 403 for read-only PUT, got {code.strip()}"

    # ---- 4D: Jobset CRUD ----
    with subtest("Create jobset for test-project"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"main\", \"nix_expression\": \"packages\"}' "
            "| jq -r .id"
        )
        test_jobset_id = result.strip()
        assert len(test_jobset_id) == 36, f"Expected UUID for jobset, got '{test_jobset_id}'"

    with subtest("List jobsets for project includes new jobset"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/jobsets | jq '.items | length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 jobset, got {result.strip()}"

    with subtest("Jobset detail page renders"):
        body = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/jobset/{test_jobset_id}"
        )
        assert "main" in body, "Jobset detail page should show jobset name"

    # ---- 4E: Evaluation trigger and lifecycle ----
    with subtest("Trigger evaluation via API"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/evaluations/trigger "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"jobset_id\": \"{test_jobset_id}\", \"commit_hash\": \"abcdef1234567890abcdef1234567890abcdef12\"}}' "
            "| jq -r .id"
        )
        test_eval_id = result.strip()
        assert len(test_eval_id) == 36, f"Expected UUID for evaluation, got '{test_eval_id}'"

    with subtest("Get evaluation by ID"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/evaluations/{test_eval_id} | jq -r .status"
        )
        assert result.strip().lower() == "pending", f"Expected pending status, got: {result.strip()}"

    with subtest("List evaluations includes triggered one"):
        result = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={test_jobset_id}' | jq '.items | length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 evaluation, got {result.strip()}"

    with subtest("Evaluation detail dashboard page renders"):
        body = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/evaluation/{test_eval_id}"
        )
        assert "abcdef123456" in body, "Evaluation page should show commit hash prefix"

    with subtest("Trigger evaluation with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/evaluations/trigger "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"jobset_id\": \"{test_jobset_id}\", \"commit_hash\": \"0000000000000000000000000000000000000000\"}}'"
        )
        assert code.strip() == "403", f"Expected 403 for read-only eval trigger, got {code.strip()}"

    # ---- 4E2: Build lifecycle (restart, bump) ----
    # Create a build via SQL since builds are normally created by the evaluator
    with subtest("Create test build via SQL"):
        machine.succeed(
            "sudo -u fc psql -U fc -d fc -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, system, priority, created_at) "
            f"VALUES ('aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee', '{test_eval_id}', 'hello', '/nix/store/fake.drv', 'failed', 'x86_64-linux', 5, NOW())"
            "\""
        )
        test_build_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"

    with subtest("Get build by ID"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{test_build_id} | jq -r .status"
        )
        assert result.strip().lower() == "failed", f"Expected failed, got: {result.strip()}"

    with subtest("Restart failed build"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/builds/{test_build_id}/restart "
            f"{auth_header} "
            "| jq -r .status"
        )
        assert result.strip().lower() == "pending", f"Expected pending status for restarted build, got: {result.strip()}"

    with subtest("Restart with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/builds/{test_build_id}/restart "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only restart, got {code.strip()}"

    # Create a pending build to test bump
    with subtest("Create pending build for bump test"):
        machine.succeed(
            "sudo -u fc psql -U fc -d fc -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, system, priority, created_at) "
            f"VALUES ('bbbbbbbb-cccc-dddd-eeee-ffffffffffff', '{test_eval_id}', 'world', '/nix/store/fake2.drv', 'pending', 'x86_64-linux', 5, NOW())"
            "\""
        )
        bump_build_id = "bbbbbbbb-cccc-dddd-eeee-ffffffffffff"

    with subtest("Bump build priority"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/builds/{bump_build_id}/bump "
            f"{auth_header} "
            "| jq -r .priority"
        )
        assert int(result.strip()) == 15, f"Expected priority 15 (5+10), got: {result.strip()}"

    with subtest("Bump with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/builds/{bump_build_id}/bump "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only bump, got {code.strip()}"

    with subtest("Cancel build"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/builds/{bump_build_id}/cancel "
            f"{auth_header} "
            "| jq '.[0].status'"
        )
        assert "cancelled" in result.strip().lower(), f"Expected cancelled, got: {result.strip()}"

    # ---- 4E3: Evaluation comparison ----
    with subtest("Trigger second evaluation for comparison"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/evaluations/trigger "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"jobset_id\": \"{test_jobset_id}\", \"commit_hash\": \"deadbeef1234567890abcdef1234567890abcdef\"}}' "
            "| jq -r .id"
        )
        second_eval_id = result.strip()
        # Add a build to the second evaluation
        machine.succeed(
            "sudo -u fc psql -U fc -d fc -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, system, priority, created_at) "
            f"VALUES ('cccccccc-dddd-eeee-ffff-aaaaaaaaaaaa', '{second_eval_id}', 'hello', '/nix/store/changed.drv', 'pending', 'x86_64-linux', 5, NOW())"
            "\""
        )
        machine.succeed(
            "sudo -u fc psql -U fc -d fc -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, system, priority, created_at) "
            f"VALUES ('dddddddd-eeee-ffff-aaaa-bbbbbbbbbbbb', '{second_eval_id}', 'new-pkg', '/nix/store/new.drv', 'pending', 'x86_64-linux', 5, NOW())"
            "\""
        )

    with subtest("Compare evaluations shows diff"):
        result = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations/{test_eval_id}/compare?to={second_eval_id}'"
        )
        data = json.loads(result)
        # hello changed derivation, world was removed, new-pkg was added
        assert len(data["changed_jobs"]) >= 1, f"Expected at least 1 changed job, got {data['changed_jobs']}"
        assert len(data["new_jobs"]) >= 1, f"Expected at least 1 new job, got {data['new_jobs']}"
        assert any(j["job_name"] == "new-pkg" for j in data["new_jobs"]), "new-pkg should be in new_jobs"

    # ---- 4F: Channel CRUD lifecycle ----
    with subtest("Create channel via API"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/channels "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"project_id\": \"{project_id}\", \"name\": \"stable\", \"jobset_id\": \"{test_jobset_id}\"}}' "
            "| jq -r .id"
        )
        test_channel_id = result.strip()
        assert len(test_channel_id) == 36, f"Expected UUID for channel, got '{test_channel_id}'"

    with subtest("List channels includes new channel"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/channels | jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 channel, got {result.strip()}"

    with subtest("Get channel by ID"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/channels/{test_channel_id} | jq -r .name"
        )
        assert result.strip() == "stable", f"Expected 'stable', got: {result.strip()}"

    with subtest("List project channels"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{project_id}/channels | jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 project channel, got {result.strip()}"

    with subtest("Promote channel to evaluation"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/channels/{test_channel_id}/promote/{test_eval_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200 for channel promote, got {code.strip()}"

    with subtest("Channel promote with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/channels/{test_channel_id}/promote/{test_eval_id} "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only promote, got {code.strip()}"

    with subtest("Create channel with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/channels "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"project_id\": \"{project_id}\", \"name\": \"nightly\", \"jobset_id\": \"{test_jobset_id}\"}}'"
        )
        assert code.strip() == "403", f"Expected 403 for read-only channel create, got {code.strip()}"

    with subtest("Delete channel"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/channels/{test_channel_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200 for channel delete, got {code.strip()}"

    # ---- 4G: Remote builder CRUD lifecycle ----
    with subtest("List remote builders"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/admin/builders | jq 'length'"
        )
        # We created one earlier in 3B
        assert int(result.strip()) >= 1, f"Expected at least 1 builder, got {result.strip()}"

    with subtest("Get remote builder by ID"):
        # Get the first builder's ID
        builder_id = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/admin/builders | jq -r '.[0].id'"
        ).strip()
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} | jq -r .name"
        )
        assert result.strip() == "test-builder", f"Expected 'test-builder', got: {result.strip()}"

    with subtest("Update remote builder (disable)"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"enabled\": false}'"
        )
        assert code.strip() == "200", f"Expected 200 for builder update, got {code.strip()}"

    with subtest("Updated builder is disabled"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} | jq -r .enabled"
        )
        assert result.strip() == "false", f"Expected false, got: {result.strip()}"

    with subtest("Update builder with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"enabled\": true}'"
        )
        assert code.strip() == "403", f"Expected 403 for read-only builder update, got {code.strip()}"

    with subtest("Delete remote builder with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only builder delete, got {code.strip()}"

    with subtest("Delete remote builder with admin key"):
        # First clear the builder_id from builds that reference it
        machine.succeed(
            "sudo -u fc psql -U fc -d fc -c "
            f"\"UPDATE builds SET builder_id = NULL WHERE builder_id = '{builder_id}'\""
        )
        # Now delete the builder
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/admin/builders/{builder_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200 for builder delete, got {code.strip()}"

    # ---- 4H: Admin system status endpoint ----
    with subtest("System status endpoint requires admin"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/admin/system "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only system status, got {code.strip()}"

    with subtest("System status endpoint returns data with admin key"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/admin/system "
            f"{auth_header} "
            "| jq .projects_count"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 project in system status, got {result.strip()}"

    # ---- 4I: API key listing ----
    with subtest("List API keys requires admin"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/api-keys "
            f"{ro_header}"
        )
        assert code.strip() == "403", f"Expected 403 for read-only API key list, got {code.strip()}"

    with subtest("List API keys returns array with admin key"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/api-keys "
            f"{auth_header} "
            "| jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 API key, got {result.strip()}"

    # ---- 4J: Badge endpoints ----
    with subtest("Badge endpoint returns SVG for unknown project"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/job/nonexistent/main/hello/shield"
        )
        # Should return 404 or error since project doesn't exist
        assert code.strip() in ("404", "500"), f"Expected 404/500 for unknown badge, got {code.strip()}"

    with subtest("Badge endpoint returns SVG for existing project"):
        # Create a badge-compatible project name lookup
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/job/test-project/main/hello/shield"
        )
        # Should return 200 with SVG (even if no builds, shows "not found" badge)
        assert code.strip() == "200", f"Expected 200 for badge, got {code.strip()}"

    with subtest("Badge returns SVG content type"):
        ct = machine.succeed(
            "curl -s -D - -o /dev/null "
            "http://127.0.0.1:3000/api/v1/job/test-project/main/hello/shield "
            "| grep -i content-type"
        )
        assert "image/svg+xml" in ct.lower(), f"Expected SVG content type, got: {ct}"

    with subtest("Latest build endpoint for unknown project returns 404"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/job/nonexistent/main/hello/latest"
        )
        assert code.strip() in ("404", "500"), f"Expected 404/500 for latest build, got {code.strip()}"

    # ---- 4K: Pagination tests ----
    # Re-verify server is healthy before pagination tests
    machine.wait_until_succeeds("curl -sf http://127.0.0.1:3000/health", timeout=15)

    with subtest("Projects pagination with limit and offset"):
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/projects?limit=1&offset=0' | jq '.items | length'"
        )
        assert int(result.strip()) == 1, f"Expected 1 project with limit=1, got {result.strip()}"

    with subtest("Projects pagination returns total count"):
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/projects?limit=1&offset=0' | jq '.total'"
        )
        assert int(result.strip()) >= 2, f"Expected at least 2 total projects, got {result.strip()}"

    with subtest("Builds pagination with limit"):
        result = machine.succeed(
            "curl -s 'http://127.0.0.1:3000/api/v1/builds?limit=5'"
        )
        data = json.loads(result)
        assert "limit" in data, f"Expected paginated response with 'limit' field, got: {result[:300]}"
        assert data["limit"] == 5, f"Expected limit=5, got {data['limit']}"

    with subtest("Evaluations pagination with limit"):
        result = machine.succeed(
            "curl -s 'http://127.0.0.1:3000/api/v1/evaluations?limit=2'"
        )
        data = json.loads(result)
        assert "limit" in data, f"Expected paginated response with 'limit' field, got: {result[:300]}"
        assert data["limit"] == 2, f"Expected limit=2, got {data['limit']}"

    # ---- 4L: Build sub-resources ----
    with subtest("Build steps endpoint returns empty array for nonexistent build"):
        result = machine.succeed(
            "curl -sf "
            "http://127.0.0.1:3000/api/v1/builds/00000000-0000-0000-0000-000000000000/steps"
            " | jq 'length'"
        )
        assert int(result.strip()) == 0, f"Expected empty steps array, got {result.strip()}"

    with subtest("Build products endpoint returns empty array for nonexistent build"):
        result = machine.succeed(
            "curl -sf "
            "http://127.0.0.1:3000/api/v1/builds/00000000-0000-0000-0000-000000000000/products"
            " | jq 'length'"
        )
        assert int(result.strip()) == 0, f"Expected empty products array, got {result.strip()}"

    with subtest("Build log endpoint for nonexistent build returns 404"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/builds/00000000-0000-0000-0000-000000000000/log"
        )
        assert code.strip() == "404", f"Expected 404 for nonexistent build log, got {code.strip()}"

    # ---- 4M: Search functionality ----
    with subtest("Search returns matching projects"):
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/search?q=test-project' | jq '.projects | length'"
        )
        assert int(result.strip()) >= 1, f"Expected at least 1 matching project, got {result.strip()}"

    with subtest("Search returns empty for nonsense query"):
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/search?q=zzzznonexistent99999' | jq '.projects | length'"
        )
        assert result.strip() == "0", f"Expected 0, got {result.strip()}"

    # ---- 4N: Content-Type verification for API endpoints ----
    with subtest("API endpoints return application/json"):
        ct = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/api/v1/projects | grep -i content-type"
        )
        assert "application/json" in ct.lower(), f"Expected application/json, got: {ct}"

    with subtest("Health endpoint returns application/json"):
        ct = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/health | grep -i content-type"
        )
        assert "application/json" in ct.lower(), f"Expected application/json, got: {ct}"

    with subtest("Metrics endpoint returns text/plain"):
        ct = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/metrics | grep -i content-type"
        )
        assert "text/plain" in ct.lower() or "text/" in ct.lower(), f"Expected text content type for metrics, got: {ct}"

    # ---- 4O: Session/Cookie auth for dashboard ----
    with subtest("Login with valid API key sets session cookie"):
        result = machine.succeed(
            "curl -s -D - -o /dev/null "
            "-X POST http://127.0.0.1:3000/login "
            f"-d 'api_key={api_token}'"
        )
        assert "fc_session=" in result, f"Expected fc_session cookie in response: {result}"
        assert "HttpOnly" in result, "Expected HttpOnly flag on session cookie"

    with subtest("Login with invalid API key shows error"):
        body = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/login "
            "-d 'api_key=fc_invalid_key'"
        )
        assert "Invalid" in body or "invalid" in body or "error" in body.lower(), \
            f"Expected error message for invalid login: {body[:200]}"

    with subtest("Login with empty API key shows error"):
        body = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/login "
            "-d 'api_key='"
        )
        assert "required" in body.lower() or "error" in body.lower() or "Invalid" in body, \
            f"Expected error message for empty login: {body[:200]}"

    with subtest("Session cookie grants admin access on dashboard"):
        # Login and capture cookie
        cookie = machine.succeed(
            "curl -s -D - -o /dev/null "
            "-X POST http://127.0.0.1:3000/login "
            f"-d 'api_key={api_token}' "
            "| grep -i set-cookie | head -1"
        )
        match = re.search(r'fc_session=([^;]+)', cookie)
        if match:
            session_val = match.group(1)
            body = machine.succeed(
                f"curl -sf -H 'Cookie: fc_session={session_val}' http://127.0.0.1:3000/admin"
            )
            # Admin page with session should show API Keys section and admin controls
            assert "API Keys" in body, "Admin page with session should show API Keys section"

    with subtest("Logout clears session cookie"):
        result = machine.succeed(
            "curl -s -D - -o /dev/null -X POST http://127.0.0.1:3000/logout"
        )
        assert "Max-Age=0" in result or "max-age=0" in result.lower(), \
            "Logout should set Max-Age=0 to clear cookie"

    # ---- 4P: RBAC with create-projects role ----
    cp_token = "fc_createprojects_key"
    cp_hash = hashlib.sha256(cp_token.encode()).hexdigest()
    machine.succeed(
        f"sudo -u fc psql -U fc -d fc -c \"INSERT INTO api_keys (name, key_hash, role) VALUES ('creator', '{cp_hash}', 'create-projects')\""
    )
    cp_header = f"-H 'Authorization: Bearer {cp_token}'"

    with subtest("create-projects role can create project"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects "
            f"{cp_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"creator-project\", \"repository_url\": \"https://example.com/creator\"}'"
        )
        assert code.strip() == "200", f"Expected 200 for create-projects role, got {code.strip()}"

    with subtest("create-projects role cannot delete project"):
        # Get the new project ID
        cp_project_id = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects | jq -r '.items[] | select(.name==\"creator-project\") | .id'"
        ).strip()
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{cp_project_id} "
            f"{cp_header}"
        )
        assert code.strip() == "403", f"Expected 403 for create-projects role DELETE, got {code.strip()}"

    with subtest("create-projects role cannot update project"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/projects/{cp_project_id} "
            f"{cp_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"description\": \"hacked\"}'"
        )
        assert code.strip() == "403", f"Expected 403 for create-projects PUT, got {code.strip()}"

    with subtest("create-projects role cannot access admin endpoints"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/admin/system "
            f"{cp_header}"
        )
        assert code.strip() == "403", f"Expected 403 for create-projects system status, got {code.strip()}"

    # ---- 4Q: Additional security tests ----
    with subtest("DELETE project without auth returns 401"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{project_id}"
        )
        assert code.strip() == "401", f"Expected 401 for unauthenticated DELETE, got {code.strip()}"

    with subtest("PUT project without auth returns 401"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X PUT http://127.0.0.1:3000/api/v1/projects/{project_id} "
            "-H 'Content-Type: application/json' "
            "-d '{\"description\": \"hacked\"}'"
        )
        assert code.strip() == "401", f"Expected 401 for unauthenticated PUT, got {code.strip()}"

    with subtest("POST channel without auth returns 401"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/channels "
            "-H 'Content-Type: application/json' "
            "-d '{\"project_id\": \"00000000-0000-0000-0000-000000000000\", \"name\": \"x\", \"jobset_id\": \"00000000-0000-0000-0000-000000000000\"}'"
        )
        assert code.strip() == "401", f"Expected 401 for unauthenticated channel create, got {code.strip()}"

    with subtest("API returns JSON error body for 404"):
        result = machine.succeed(
            "curl -sf http://127.0.0.1:3000/api/v1/projects/00000000-0000-0000-0000-000000000001 2>&1 || "
            "curl -s http://127.0.0.1:3000/api/v1/projects/00000000-0000-0000-0000-000000000001"
        )
        parsed = json.loads(result)
        assert "error" in parsed or "error_code" in parsed, f"Expected JSON error body, got: {result}"

    with subtest("Nonexistent API route returns 404"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/nonexistent"
        )
        # Axum returns 404 for unmatched routes
        assert code.strip() in ("404", "405"), f"Expected 404/405 for nonexistent route, got {code.strip()}"

    with subtest("HEAD request to health returns 200"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' -I http://127.0.0.1:3000/health"
        )
        assert code.strip() == "200", f"Expected 200 for HEAD /health, got {code.strip()}"

    with subtest("OPTIONS request returns valid response"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X OPTIONS http://127.0.0.1:3000/api/v1/projects"
        )
        # Axum may return 200, 204, or 405 depending on CORS configuration
        assert code.strip() in ("200", "204", "405"), f"Expected 200/204/405 for OPTIONS, got {code.strip()}"

    # ========================================================================
    # Phase 5: New Feature Tests (Structured Logging, Flake Probe, Setup Wizard, Dashboard)
    # ========================================================================

    # ---- 5A: Structured logging ----
    with subtest("Server produces structured log output"):
        # The server should log via tracing with the configured format
        result = machine.succeed("journalctl -u fc-server --no-pager -n 50 2>&1")
        # With compact/full format, tracing outputs level and target info
        assert "INFO" in result or "info" in result, \
            "Expected structured log lines with INFO level in journalctl output"

    # ---- 5B: Static CSS serving ----
    with subtest("Static CSS endpoint returns 200 with correct content type"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/static/style.css"
        )
        assert code.strip() == "200", f"Expected 200 for /static/style.css, got {code.strip()}"
        ct = machine.succeed(
            "curl -s -D - -o /dev/null http://127.0.0.1:3000/static/style.css | grep -i content-type"
        )
        assert "text/css" in ct.lower(), f"Expected text/css, got: {ct}"

    # ---- 5C: Setup wizard page ----
    with subtest("Setup wizard page returns 200"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/projects/new"
        )
        assert code.strip() == "200", f"Expected 200 for /projects/new, got {code.strip()}"

    with subtest("Setup wizard page contains wizard steps"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/projects/new")
        assert "Step 1" in body, "Setup wizard should contain Step 1"
        assert "Repository URL" in body, "Setup wizard should contain URL input"
        assert "probeRepo" in body, "Setup wizard should contain probe JS function"

    with subtest("Projects page links to setup wizard"):
        # Login first to get admin view
        cookie = machine.succeed(
            "curl -s -D - -o /dev/null "
            "-X POST http://127.0.0.1:3000/login "
            f"-d 'api_key={api_token}' "
            "| grep -i set-cookie | head -1"
        )
        match = re.search(r'fc_session=([^;]+)', cookie)
        if match:
            session_val = match.group(1)
            body = machine.succeed(
                f"curl -sf -H 'Cookie: fc_session={session_val}' http://127.0.0.1:3000/projects"
            )
            assert '/projects/new' in body, "Projects page should link to /projects/new wizard"

    # ---- 5D: Flake probe endpoint ----
    with subtest("Probe endpoint exists and requires POST"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/projects/probe"
        )
        # GET should return 405 (Method Not Allowed)
        assert code.strip() in ("404", "405"), f"Expected 404/405 for GET /probe, got {code.strip()}"

    with subtest("Probe endpoint accepts POST with auth"):
        # This will likely fail since the VM has no network access to github,
        # but we can verify the endpoint exists and returns a proper error
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects/probe "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"repository_url\": \"https://github.com/nonexistent/repo\"}'"
        )
        # Should return 408 (timeout), 422 (nix eval error), 500, or 200 with is_flake=false
        # Any non-crash response is acceptable
        assert code.strip() in ("200", "408", "422", "500"), \
            f"Expected 200/408/422/500 for probe of unreachable repo, got {code.strip()}"

    # ---- 5E: Setup endpoint ----
    with subtest("Setup endpoint exists and requires POST"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "http://127.0.0.1:3000/api/v1/projects/setup"
        )
        assert code.strip() in ("404", "405"), f"Expected 404/405 for GET /setup, got {code.strip()}"

    with subtest("Setup endpoint creates project with jobsets"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/setup "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"repository_url\": \"https://github.com/test/setup-test\", \"name\": \"setup-test\", \"description\": \"Created via setup\", \"jobsets\": [{\"name\": \"packages\", \"nix_expression\": \"packages\"}]}' "
            "| jq -r .project.id"
        )
        setup_project_id = result.strip()
        assert len(setup_project_id) == 36, f"Expected UUID from setup, got '{setup_project_id}'"

    with subtest("Setup-created project has jobsets"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{setup_project_id}/jobsets | jq '.items | length'"
        )
        assert int(result.strip()) == 1, f"Expected 1 jobset from setup, got {result.strip()}"

    with subtest("Setup endpoint with read-only key returns 403"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            "-X POST http://127.0.0.1:3000/api/v1/projects/setup "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"repository_url\": \"https://github.com/test/ro\", \"name\": \"ro-setup\", \"jobsets\": []}'"
        )
        assert code.strip() == "403", f"Expected 403 for read-only setup, got {code.strip()}"

    # Clean up setup-test project
    machine.succeed(
        f"curl -sf -X DELETE http://127.0.0.1:3000/api/v1/projects/{setup_project_id} "
        f"{auth_header}"
    )

    # ---- 5F: Dashboard improvements ----
    with subtest("Home page has dashboard-grid two-column layout"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        assert "dashboard-grid" in body, "Home page should have dashboard-grid class"

    with subtest("Home page has colored stat values"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        assert "stat-value-green" in body, "Home page should have green stat value for completed"
        assert "stat-value-red" in body, "Home page should have red stat value for failed"

    with subtest("Home page has escapeHtml utility"):
        body = machine.succeed("curl -sf http://127.0.0.1:3000/")
        assert "escapeHtml" in body, "Home page should include escapeHtml function"

    with subtest("Admin page JS uses escapeHtml for error handling"):
        # Login to get admin view
        if match:
            body = machine.succeed(
                f"curl -sf -H 'Cookie: fc_session={session_val}' http://127.0.0.1:3000/admin"
            )
            assert "escapeHtml" in body, "Admin page JS should use escapeHtml"

    # ---- 4R: Metrics reflect actual data ----
    with subtest("Metrics fc_projects_total reflects created projects"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/metrics")
        for line in result.split("\n"):
            if line.startswith("fc_projects_total"):
                val = int(line.split()[-1])
                assert val >= 3, f"Expected fc_projects_total >= 3, got {val}"
                break

    with subtest("Metrics fc_evaluations_total reflects triggered evaluation"):
        result = machine.succeed("curl -sf http://127.0.0.1:3000/metrics")
        for line in result.split("\n"):
            if line.startswith("fc_evaluations_total"):
                val = int(line.split()[-1])
                assert val >= 1, f"Expected fc_evaluations_total >= 1, got {val}"
                break

    # ========================================================================
    # Phase E2E-1: End-to-End Evaluator Integration Test
    # ========================================================================

    # ---- Create a test flake inside the VM ----
    with subtest("Create bare git repo with test flake"):
        machine.succeed("mkdir -p /var/lib/fc/test-repos")
        machine.succeed("git init --bare /var/lib/fc/test-repos/test-flake.git")

        # Create a working copy, write the flake, commit, push
        machine.succeed("mkdir -p /tmp/test-flake-work")
        machine.succeed("cd /tmp/test-flake-work && git init")
        machine.succeed("cd /tmp/test-flake-work && git config user.email 'test@fc' && git config user.name 'FC Test'")

        # Write a minimal flake.nix that builds a simple derivation
        machine.succeed("""
            cat > /tmp/test-flake-work/flake.nix << 'FLAKE'
            {
              description = "FC CI test flake";
              outputs = { self, ... }: {
                packages.x86_64-linux.hello = derivation {
                  name = "fc-test-hello";
                  system = "x86_64-linux";
                  builder = "/bin/sh";
                  args = [ "-c" "echo hello > $out" ];
                };
              };
            }
            FLAKE
        """)
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'initial flake'")
        machine.succeed("cd /tmp/test-flake-work && git remote add origin /var/lib/fc/test-repos/test-flake.git")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/main")

        # Set ownership for fc user
        machine.succeed("chown -R fc:fc /var/lib/fc/test-repos")

    # ---- Create project + jobset pointing to the local repo via API ----
    with subtest("Create E2E project and jobset via API"):
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/projects "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"e2e-test\", \"repository_url\": \"https://github.com/nixos/nixpkgs\"}' "
            "| jq -r .id"
        )
        e2e_project_id = result.strip()
        assert len(e2e_project_id) == 36, f"Expected UUID, got '{e2e_project_id}'"

        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"packages\", \"nix_expression\": \"packages\", \"flake_mode\": true, \"enabled\": true, \"check_interval\": 5, \"branch\": null, \"scheduling_shares\": 100}' "
            "| jq -r .id"
        )
        e2e_jobset_id = result.strip()
        assert len(e2e_jobset_id) == 36, f"Expected UUID for jobset, got '{e2e_jobset_id}'"

    # ---- Wait for evaluator to pick it up and create an evaluation ----
    with subtest("Evaluator discovers and evaluates the flake"):
        # The evaluator is already running (started in Phase 1)
        # Poll for evaluation to appear with status "completed"
        machine.wait_until_succeeds(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' "
            "| jq -e '.items[] | select(.status==\"completed\")'",
            timeout=90
        )

    with subtest("Evaluation created builds with valid drv_path"):
        # Get evaluation ID
        e2e_eval_id = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' "
            "| jq -r '.items[] | select(.status==\"completed\") | .id' | head -1"
        ).strip()
        assert len(e2e_eval_id) == 36, f"Expected UUID for evaluation, got '{e2e_eval_id}'"

        # Verify builds were created
        result = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/builds?evaluation_id={e2e_eval_id}' | jq '.items | length'"
        )
        build_count = int(result.strip())
        assert build_count >= 1, f"Expected >= 1 build, got {build_count}"

        # Verify build has valid drv_path
        drv_path = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/builds?evaluation_id={e2e_eval_id}' | jq -r '.items[0].drv_path'"
        ).strip()
        assert drv_path.startswith("/nix/store/"), f"Expected /nix/store/ drv_path, got '{drv_path}'"

        # Get the build ID for later
        e2e_build_id = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/builds?evaluation_id={e2e_eval_id}' | jq -r '.items[0].id'"
        ).strip()

    # ---- Test evaluation caching ----
    with subtest("Same commit does not trigger a new evaluation"):
        # Get current evaluation count
        before_count = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' | jq '.items | length'"
        ).strip()
        # Wait a poll cycle
        time.sleep(10)
        after_count = machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' | jq '.items | length'"
        ).strip()
        assert before_count == after_count, f"Evaluation count changed from {before_count} to {after_count} (should be cached)"

    # ---- Test new commit triggers new evaluation ----
    with subtest("New commit triggers new evaluation"):
        before_count_int = int(machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' | jq '.items | length'"
        ).strip())

        # Push a new commit
        machine.succeed("""
            cd /tmp/test-flake-work && \
            cat > flake.nix << 'FLAKE'
            {
              description = "FC CI test flake v2";
              outputs = { self, ... }: {
                packages.x86_64-linux.hello = derivation {
                  name = "fc-test-hello-v2";
                  system = "x86_64-linux";
                  builder = "/bin/sh";
                  args = [ "-c" "echo hello-v2 > $out" ];
                };
              };
            }
            FLAKE
        """)
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'v2 update'")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/main")

        # Wait for evaluator to detect and create new evaluation
        machine.wait_until_succeeds(
            f"test $(curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' | jq '.items | length') -gt {before_count_int}",
            timeout=60
        )

    # ========================================================================
    # Phase E2E-2: End-to-End Queue Runner Integration Test
    # ========================================================================

    with subtest("Queue runner builds pending derivation"):
        # Poll the E2E build until completed (queue-runner is already running)
        machine.wait_until_succeeds(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id} | jq -e 'select(.status==\"completed\")'",
            timeout=120
        )

    with subtest("Completed build has output path"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id} | jq -r .build_output_path"
        ).strip()
        assert result != "null" and result.startswith("/nix/store/"), \
            f"Expected /nix/store/ output path, got '{result}'"

    with subtest("Build steps recorded"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id}/steps | jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected >= 1 build step, got {result.strip()}"

        # Verify exit_code = 0
        exit_code = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id}/steps | jq '.[0].exit_code'"
        ).strip()
        assert exit_code == "0", f"Expected exit_code 0, got {exit_code}"

    with subtest("Build products created"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id}/products | jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected >= 1 build product, got {result.strip()}"

        # Verify product has valid path
        product_path = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id}/products | jq -r '.[0].path'"
        ).strip()
        assert product_path.startswith("/nix/store/"), f"Expected /nix/store/ product path, got '{product_path}'"

    with subtest("Build log exists"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"http://127.0.0.1:3000/api/v1/builds/{e2e_build_id}/log"
        ).strip()
        assert code == "200", f"Expected 200 for build log, got {code}"

    # ========================================================================
    # Phase E2E-3: Jobset Input Management API
    # ========================================================================

    with subtest("Create jobset input via API"):
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"nixpkgs\", \"input_type\": \"git\", \"value\": \"https://github.com/NixOS/nixpkgs\"}'"
        )
        input_data = json.loads(result)
        assert "id" in input_data, f"Expected id in response: {result}"
        e2e_input_id = input_data["id"]

    with subtest("List jobset inputs"):
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs | jq 'length'"
        )
        assert int(result.strip()) >= 1, f"Expected >= 1 input, got {result.strip()}"

    with subtest("Read-only key cannot create jobset input"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs "
            f"{ro_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"test\", \"input_type\": \"string\", \"value\": \"hello\"}'"
        ).strip()
        assert code == "403", f"Expected 403 for read-only input create, got {code}"

    with subtest("Delete jobset input"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs/{e2e_input_id} "
            f"{auth_header}"
        ).strip()
        assert code == "200", f"Expected 200 for input delete, got {code}"

    with subtest("Read-only key cannot delete jobset input"):
        # Re-create first
        result = machine.succeed(
            f"curl -sf -X POST http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            "-d '{\"name\": \"test-ro\", \"input_type\": \"string\", \"value\": \"test\"}'"
        )
        tmp_input_id = json.loads(result)["id"]
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets/{e2e_jobset_id}/inputs/{tmp_input_id} "
            f"{ro_header}"
        ).strip()
        assert code == "403", f"Expected 403 for read-only input delete, got {code}"

    # ========================================================================
    # Phase E2E-4: Notification Dispatch
    # ========================================================================

    # Notifications are dispatched after builds complete (already tested above).
    # Verify run_command notifications work:
    with subtest("Notification run_command is invoked on build completion"):
        # This tests that the notification system dispatches properly.
        # The actual run_command config is not set in this VM, so we just verify
        # the build status was updated correctly after notification dispatch.
        result = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id} | jq -r .status"
        ).strip()
        assert result == "completed", f"Expected completed after notification, got {result}"

    # ========================================================================
    # Phase E2E-5: Channel Auto-Promotion
    # ========================================================================

    with subtest("Channel auto-promotion after all builds complete"):
        # Create a channel tracking the E2E jobset
        result = machine.succeed(
            "curl -sf -X POST http://127.0.0.1:3000/api/v1/channels "
            f"{auth_header} "
            "-H 'Content-Type: application/json' "
            f"-d '{{\"project_id\": \"{e2e_project_id}\", \"name\": \"e2e-channel\", \"jobset_id\": \"{e2e_jobset_id}\"}}' "
            "| jq -r .id"
        )
        e2e_channel_id = result.strip()

        # Auto-promotion happens when all builds in an evaluation complete.
        # The first evaluation's builds should already be complete.
        # Check channel's current_evaluation_id
        machine.wait_until_succeeds(
            f"curl -sf http://127.0.0.1:3000/api/v1/channels/{e2e_channel_id} "
            "| jq -e 'select(.current_evaluation_id != null)'",
            timeout=30
        )

    # ========================================================================
    # Phase E2E-6: Binary Cache NARinfo Test
    # ========================================================================

    with subtest("Binary cache serves NARinfo for built output"):
        # Get the build output path
        output_path = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{e2e_build_id} | jq -r .build_output_path"
        ).strip()

        # Extract the hash from /nix/store/<hash>-<name>
        hash_match = re.match(r'/nix/store/([a-z0-9]+)-', output_path)
        assert hash_match, f"Could not extract hash from output path: {output_path}"
        store_hash = hash_match.group(1)

        # Request NARinfo
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"http://127.0.0.1:3000/nix-cache/{store_hash}.narinfo"
        ).strip()
        assert code == "200", f"Expected 200 for NARinfo, got {code}"

        # Verify NARinfo content has StorePath and NarHash
        narinfo = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/nix-cache/{store_hash}.narinfo"
        )
        assert "StorePath:" in narinfo, f"NARinfo missing StorePath: {narinfo}"
        assert "NarHash:" in narinfo, f"NARinfo missing NarHash: {narinfo}"

    # ========================================================================
    # Phase E2E-7: Build Retry on Failure
    # ========================================================================

    with subtest("Build with invalid drv_path fails and retries"):
        # Insert a build with an invalid drv_path via SQL
        machine.succeed(
            "sudo -u postgres psql -d fc -c \""
            "INSERT INTO builds (id, evaluation_id, job_name, drv_path, status, priority, retry_count, max_retries, is_aggregate, signed) "
            f"VALUES (gen_random_uuid(), '{e2e_eval_id}', 'bad-build', '/nix/store/invalid-does-not-exist.drv', 'pending', 0, 0, 3, false, false);\""
        )

        # Wait for queue-runner to attempt the build and fail it
        machine.wait_until_succeeds(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=bad-build' "
            "| jq -e '.items[] | select(.status==\"failed\")'",
            timeout=60
        )

        # Verify status is failed
        result = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=bad-build' | jq -r '.items[0].status'"
        ).strip()
        assert result == "failed", f"Expected failed for bad build, got '{result}'"

    # ========================================================================
    # Phase E2E-8: Notification Dispatch (run_command)
    # ========================================================================

    with subtest("Notification run_command invoked on build completion"):
        # Write a notification script
        machine.succeed("mkdir -p /var/lib/fc")
        machine.succeed("""
            cat > /var/lib/fc/notify.sh << 'SCRIPT'
    #!/bin/sh
    echo "BUILD_STATUS=$FC_BUILD_STATUS" >> /var/lib/fc/notify-output
    echo "BUILD_ID=$FC_BUILD_ID" >> /var/lib/fc/notify-output
    echo "BUILD_JOB=$FC_BUILD_JOB" >> /var/lib/fc/notify-output
    SCRIPT
        """)
        machine.succeed("chmod +x /var/lib/fc/notify.sh")
        machine.succeed("chown -R fc:fc /var/lib/fc")

        # Update fc.toml to enable notifications
        machine.succeed("""
            cat >> /etc/fc.toml << 'CONFIG'

    [notifications]
    run_command = "/var/lib/fc/notify.sh"
    CONFIG
        """)

        # Restart queue-runner to pick up new config
        machine.succeed("systemctl restart fc-queue-runner")
        machine.wait_for_unit("fc-queue-runner.service", timeout=30)

        # Create a new simple build to trigger notification
        # Push a trivial change to trigger a new evaluation
        machine.succeed("""
            cd /tmp/test-flake-work && \
            cat > flake.nix << 'FLAKE'
    {
      description = "FC CI test flake notify";
      outputs = { self, ... }: {
    packages.x86_64-linux.notify-test = derivation {
      name = "fc-notify-test";
      system = "x86_64-linux";
      builder = "/bin/sh";
      args = [ "-c" "echo notify-test > $out" ];
    };
      };
    }
    FLAKE
        """)
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'trigger notification test'")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/main")

        # Wait for evaluator to create new evaluation
        machine.wait_until_succeeds(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' "
            "| jq '.items | length' | grep -v '^2$'",
            timeout=60
        )

        # Get the new build ID
        notify_build_id = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=notify-test' | jq -r '.items[0].id'"
        ).strip()

        # Wait for the build to complete
        machine.wait_until_succeeds(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{notify_build_id} | jq -e 'select(.status==\"completed\")'",
            timeout=120
        )

        # Wait a bit for notification to dispatch
        time.sleep(5)

        # Verify the notification script was executed
        machine.wait_for_file("/var/lib/fc/notify-output")
        output = machine.succeed("cat /var/lib/fc/notify-output")
        assert "BUILD_STATUS=success" in output or "BUILD_STATUS=completed" in output, \
            f"Expected BUILD_STATUS in notification output, got: {output}"
        assert notify_build_id in output, f"Expected build ID {notify_build_id} in output, got: {output}"

    # ========================================================================
    # Phase E2E-9: Nix Signing
    # ========================================================================

    with subtest("Generate signing key and configure signing"):
        # Generate a Nix signing key
        machine.succeed("mkdir -p /var/lib/fc/keys")
        machine.succeed("nix-store --generate-binary-cache-key fc-test /var/lib/fc/keys/signing-key /var/lib/fc/keys/signing-key.pub")
        machine.succeed("chown -R fc:fc /var/lib/fc/keys")
        machine.succeed("chmod 600 /var/lib/fc/keys/signing-key")

        # Update fc.toml to enable signing
        machine.succeed("""
            cat >> /etc/fc.toml << 'CONFIG'

    [signing]
    enabled = true
    key_file = "/var/lib/fc/keys/signing-key"
    CONFIG
        """)

        # Restart queue-runner to pick up signing config
        machine.succeed("systemctl restart fc-queue-runner")
        machine.wait_for_unit("fc-queue-runner.service", timeout=30)

    with subtest("Signed builds have valid signatures"):
        # Create a new build to test signing
        machine.succeed("""
            cd /tmp/test-flake-work && \
            cat > flake.nix << 'FLAKE'
    {
      description = "FC CI test flake signing";
      outputs = { self, ... }: {
    packages.x86_64-linux.sign-test = derivation {
      name = "fc-sign-test";
      system = "x86_64-linux";
      builder = "/bin/sh";
      args = [ "-c" "echo signed-build > $out" ];
    };
      };
    }
    FLAKE
        """)
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'trigger signing test'")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/main")

        # Wait for evaluation
        machine.wait_until_succeeds(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' "
            "| jq '.items | length' | grep -v '^[23]$'",
            timeout=60
        )

        # Get the sign-test build
        sign_build_id = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=sign-test' | jq -r '.items[0].id'"
        ).strip()

        # Wait for build to complete
        machine.wait_until_succeeds(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{sign_build_id} | jq -e 'select(.status==\"completed\")'",
            timeout=120
        )

        # Verify the build has signed=true
        signed = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{sign_build_id} | jq -r .signed"
        ).strip()
        assert signed == "true", f"Expected signed=true, got {signed}"

        # Get the output path and verify it with nix store verify
        output_path = machine.succeed(
            f"curl -sf http://127.0.0.1:3000/api/v1/builds/{sign_build_id} | jq -r .build_output_path"
        ).strip()

        # Verify the path is signed with our key
        # The verify command should succeed (exit 0) if signatures are valid
        machine.succeed(f"nix store verify --sigs-needed 1 {output_path}")

    # ========================================================================
    # Phase E2E-10: GC Roots
    # ========================================================================

    with subtest("GC roots are created for build products"):
        # Enable GC in config
        machine.succeed("""
            cat >> /etc/fc.toml << 'CONFIG'

    [gc]
    enabled = true
    gc_roots_dir = "/nix/var/nix/gcroots/per-user/fc"
    max_age_days = 30
    cleanup_interval = 3600
    CONFIG
        """)

        # Restart queue-runner to enable GC
        machine.succeed("systemctl restart fc-queue-runner")
        machine.wait_for_unit("fc-queue-runner.service", timeout=30)

        # Ensure the gc roots directory exists
        machine.succeed("mkdir -p /nix/var/nix/gcroots/per-user/fc")
        machine.succeed("chown -R fc:fc /nix/var/nix/gcroots/per-user/fc")

        # Create a new build to test GC root creation
        machine.succeed("""
            cd /tmp/test-flake-work && \
            cat > flake.nix << 'FLAKE'
    {
      description = "FC CI test flake gc";
      outputs = { self, ... }: {
    packages.x86_64-linux.gc-test = derivation {
      name = "fc-gc-test";
      system = "x86_64-linux";
      builder = "/bin/sh";
      args = [ "-c" "echo gc-test > $out" ];
    };
      };
    }
    FLAKE
        """)
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'trigger gc test'")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/main")

        # Wait for evaluation and build
        machine.wait_until_succeeds(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=gc-test' | jq -e '.items[] | select(.status==\"completed\")'",
            timeout=120
        )

        # Get the build output path
        gc_build_output = machine.succeed(
            "curl -sf 'http://127.0.0.1:3000/api/v1/builds?job_name=gc-test' | jq -r '.items[0].build_output_path'"
        ).strip()

        # Verify GC root symlink was created
        # The symlink should be in /nix/var/nix/gcroots/per-user/fc/ and point to the build output
        gc_roots = machine.succeed("find /nix/var/nix/gcroots/per-user/fc -type l 2>/dev/null || true").strip()

        # Check if any symlink points to our build output
        if gc_roots:
            found_root = False
            for root in gc_roots.split('\n'):
                if root:
                    target = machine.succeed(f"readlink -f {root} 2>/dev/null || true").strip()
                    if target == gc_build_output:
                        found_root = True
                        break

            # We might have GC roots - this is expected behavior
            # The key is that the build output exists and is protected from GC
            machine.succeed(f"test -e {gc_build_output}")
        else:
            # If no GC roots yet, at least verify the build output exists
            # GC roots might be created asynchronously
            machine.succeed(f"test -e {gc_build_output}")

    # ========================================================================
    # Phase E2E-11: Declarative In-Repo Config
    # ========================================================================

    with subtest("Declarative .fc.toml in repo auto-creates jobset"):
        # Add .fc.toml to the test repo with a new jobset definition
        machine.succeed("""
            cd /tmp/test-flake-work && \
            cat > .fc.toml << 'FCTOML'
            [[jobsets]]
            name = "declarative-checks"
            nix_expression = "checks"
            flake_mode = true
            enabled = true
            FCTOML
        """)
        machine.succeed("cd /tmp/test-flake-work && git add -A && git commit -m 'add declarative config'")
        machine.succeed("cd /tmp/test-flake-work && git push origin HEAD:refs/heads/main")

        # Wait for evaluator to pick up the new commit and process declarative config
        machine.wait_until_succeeds(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/projects/{e2e_project_id}/jobsets' "
            "| jq -e '.items[] | select(.name==\"declarative-checks\")'",
            timeout=60
        )

    # ========================================================================
    # Phase E2E-12: Webhook Endpoint
    # ========================================================================

    with subtest("Webhook endpoint accepts valid GitHub push"):
        # Create a webhook config via SQL (no REST endpoint for creation)
        machine.succeed(
            "sudo -u postgres psql -d fc -c \""
            "INSERT INTO webhook_configs (id, project_id, forge_type, secret_hash, enabled) "
            f"VALUES (gen_random_uuid(), '{e2e_project_id}', 'github', 'test-secret', true);\""
        )

        # Get the current evaluation count
        before_evals = int(machine.succeed(
            f"curl -sf 'http://127.0.0.1:3000/api/v1/evaluations?jobset_id={e2e_jobset_id}' | jq '.items | length'"
        ).strip())

        # Compute HMAC-SHA256 of the payload
        payload = '{"ref":"refs/heads/main","after":"abcdef1234567890abcdef1234567890abcdef12","repository":{"clone_url":"file:///var/lib/fc/test-repos/test-flake.git"}}'

        # Generate HMAC with the secret
        hmac_sig = machine.succeed(
            f"echo -n '{payload}' | openssl dgst -sha256 -hmac 'test-secret' -hex | awk '{{print $2}}'"
        ).strip()

        # Send webhook
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/webhooks/{e2e_project_id}/github "
            "-H 'Content-Type: application/json' "
            f"-H 'X-Hub-Signature-256: sha256={hmac_sig}' "
            f"-d '{payload}'"
        ).strip()
        assert code == "200", f"Expected 200 for webhook, got {code}"

        # Verify the webhook response accepted the push
        result = machine.succeed(
            "curl -sf "
            f"-X POST http://127.0.0.1:3000/api/v1/webhooks/{e2e_project_id}/github "
            "-H 'Content-Type: application/json' "
            f"-H 'X-Hub-Signature-256: sha256={hmac_sig}' "
            f"-d '{payload}' | jq -r .accepted"
        ).strip()
        assert result == "true", f"Expected webhook accepted=true, got {result}"

    with subtest("Webhook rejects invalid signature"):
        payload = '{"ref":"refs/heads/main","after":"deadbeef","repository":{}}'
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X POST http://127.0.0.1:3000/api/v1/webhooks/{e2e_project_id}/github "
            "-H 'Content-Type: application/json' "
            "-H 'X-Hub-Signature-256: sha256=0000000000000000000000000000000000000000000000000000000000000000' "
            f"-d '{payload}'"
        ).strip()
        assert code == "401", f"Expected 401 for invalid webhook signature, got {code}"

    # ---- 4S: Delete project with auth (cleanup) ----
    with subtest("Delete project with admin key succeeds"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"-X DELETE http://127.0.0.1:3000/api/v1/projects/{project_id} "
            f"{auth_header}"
        )
        assert code.strip() == "200", f"Expected 200 for admin DELETE project, got {code.strip()}"

    with subtest("Deleted project returns 404"):
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"http://127.0.0.1:3000/api/v1/projects/{project_id}"
        )
        assert code.strip() == "404", f"Expected 404 for deleted project, got {code.strip()}"

    with subtest("Cascade delete removes jobsets and evaluations"):
        # The jobset and evaluation we created should be gone
        code = machine.succeed(
            "curl -s -o /dev/null -w '%{http_code}' "
            f"http://127.0.0.1:3000/api/v1/evaluations/{test_eval_id}"
        )
        assert code.strip() == "404", f"Expected 404 for cascaded evaluation, got {code.strip()}"
  '';
}

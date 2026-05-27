# Circus

[design document]: ./DESIGN.md

Circus is a Rust-based continuous integration system built from the ground up
for Nix-based projects. It follows Hydra's three-daemon architecture while
addressing its operational pain points: performance, maintainability, and
declarative configuration.

Heavily work in progress. Documentation is still scattered, and may not reflect
the latest state of the project until it is deemed complete. Please create an
issue if you notice and obvious inaccuracy. While I not guarantee a quick
response, I'd appreciate the heads-up. PRs are also very welcome.

## Architecture

Circus follows Hydra's three-daemon model with a shared PostgreSQL database:

- **server** (`circus-server`): REST API (Axum), dashboard, binary cache,
  metrics, webhooks
- **evaluator** (`circus-evaluator`): Git polling and Nix evaluation via
  `nix-eval-jobs`
- **queue-runner** (`circus-queue-runner`): Build dispatch with semaphore-based
  worker pool
- **common** (`circus-common`): Shared types, database layer, configuration,
  validation
- **migrate-cli** (`circus-migrate`): Database migration utility

```mermaid
flowchart LR
    A["Git Repo"] --> B["Evaluator<br/>(polls, clones, runs nix-eval-jobs)"]
    B --> C["Evaluation + Build Records<br/>in DB"]
    C --> D["Queue Runner<br/>(claims builds atomically,<br/>runs nix build)"]
    D --> E["BuildSteps<br/>and BuildProducts"]
```

See the [design document] for more details on the architecture, similarities and
differences with Hydra. For feedback and questions, head to the issues tab.

## Development

```bash
nix develop          # Enter dev shell (rust-analyzer, postgresql, pkg-config, openssl)
cargo build          # Build all crates
cargo test           # Run all tests
cargo check          # Type-check only
```

Build a specific crate:

```bash
cargo build -p circus-server
cargo build -p circus-evaluator
cargo build -p circus-queue-runner
cargo build -p circus-common
cargo build -p circus-migrate-cli
```

## Quick Start

1. Enter dev shell and start PostgreSQL:

   ```bash
   nix develop
   initdb -D /tmp/circus-pg
   pg_ctl -D /tmp/circus-pg start
   createuser circus
   createdb -O circus circus
   ```

2. Run migrations:

   ```bash
   cargo run --bin circus-migrate -- up postgresql://circus@localhost/circus
   ```

3. Start the server:

   ```bash
   CIRCUS_DATABASE__URL=postgresql://circus@localhost/circus cargo run --bin circus-server
   ```

4. Open <http://localhost:3000> in your browser.

## Demo VM

A self-contained NixOS VM is available for trying Circus without any manual
setup. It runs `circus-server` with PostgreSQL, seeds demo API keys, and
forwards port 3000 to the host.

### Running

```bash
# Build the demo VM
$ nix build .#demo-vm

# Run the demo VM
$ ./result/bin/run-circus-demo-vm
```

The VM boots to a serial console (no graphical display). Once the boot
completes, the server is reachable from your host at `http://localhost:3000`.

### Pre-seeded Credentials

To make the testing process easier, an admin key and a read-only API key are
pre-seeded in the demo VM. This should let you test a majority of features
without having to set up an account each time you spin up your VM.

| Key                        | Role        | Use for                      |
| -------------------------- | ----------- | ---------------------------- |
| `circus_demo_admin_key`    | `admin`     | Full access, dashboard login |
| `circus_demo_readonly_key` | `read-only` | Read-only API access         |

Log in to the dashboard at `http://localhost:3000/login` using the admin key.

### Example API Calls

Circus is designed as a server in mind, and the dashboard is a convenient
wrapper around the API. If you are testing with new routes you may test them
with curl without ever spinning up a browser:

<!--markdownlint-disable MD013-->

```bash
# Health check
curl -s http://localhost:3000/health | jq

# Create a project
curl -s -X POST http://localhost:3000/api/v1/projects \
  -H 'Authorization: Bearer circus_demo_admin_key' \
  -H 'Content-Type: application/json' \
  -d '{"name": "my-project", "repository_url": "https://github.com/NixOS/nixpkgs"}' | jq

# List projects
curl -s http://localhost:3000/api/v1/projects | jq

# Try with read-only key (write should fail with 403)
curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:3000/api/v1/projects \
  -H 'Authorization: Bearer circus_demo_readonly_key' \
  -H 'Content-Type: application/json' \
  -d '{"name": "should-fail", "repository_url": "https://example.com"}'
```

<!--markdownlint-enable MD013-->

### Inside the VM

The serial console auto-logs in as root. While in the VM, you may use the TTY
access to investigate server logs or make API calls.

```bash
# Useful commands:
$ systemctl status circus-server
$ journalctl -u circus-server -f          # Live server logs
$ curl -sf localhost:3000/health | jq # Health status
$ curl -sf localhost:3000/metrics     # Metrics
```

Press `Ctrl-a x` to shut down QEMU.

### VM Options

The VM uses QEMU user-mode networking. If port 3000 conflicts on your host, you
can override the QEMU options:

```bash
QEMU_NET_OPTS="hostfwd=tcp::8080-:3000" ./result/bin/run-circus-demo-vm
```

This makes the dashboard available at <http://localhost:8080> instead.

## Configuration

Circus reads configuration from a TOML file with environment variable overrides.
Override hierarchy (highest wins):

1. Compiled defaults
2. `circus.toml` in working directory
3. File at `CIRCUS_CONFIG_FILE` env var
4. `CIRCUS_*` env vars (`__` as nested separator, e.g. `CIRCUS_DATABASE__URL`)

See `circus.toml` in the repository root for the full schema with comments.

### Configuration Reference

A somewhat maintained list of configuration options. Might be outdated during
development.

<!--markdownlint-disable MD013 -->

| Section         | Key                    | Default                                             | Description                                           |
| --------------- | ---------------------- | --------------------------------------------------- | ----------------------------------------------------- |
| `database`      | `url`                  | `postgresql://circus:password@localhost/circus`     | PostgreSQL connection URL                             |
| `database`      | `max_connections`      | `20`                                                | Maximum connection pool size                          |
| `database`      | `min_connections`      | `5`                                                 | Minimum idle connections                              |
| `database`      | `connect_timeout`      | `30`                                                | Connection timeout (seconds)                          |
| `database`      | `idle_timeout`         | `600`                                               | Idle connection timeout (seconds)                     |
| `database`      | `max_lifetime`         | `1800`                                              | Maximum connection lifetime (seconds)                 |
| `server`        | `host`                 | `127.0.0.1`                                         | HTTP listen address                                   |
| `server`        | `port`                 | `3000`                                              | HTTP listen port                                      |
| `server`        | `request_timeout`      | `30`                                                | Per-request timeout (seconds)                         |
| `server`        | `max_body_size`        | `10485760`                                          | Maximum request body size (10 MB)                     |
| `server`        | `api_key`              | none                                                | Optional legacy API key (prefer DB keys)              |
| `server`        | `cors_permissive`      | `false`                                             | Allow all CORS origins                                |
| `server`        | `allowed_origins`      | `[]`                                                | Allowed CORS origins list                             |
| `server`        | `force_secure_cookies` | `false`                                             | Force Secure flag on cookies (enable for HTTPS proxy) |
| `server`        | `rate_limit_rps`       | none                                                | Requests per second limit per IP (DoS protection)     |
| `server`        | `rate_limit_burst`     | none                                                | Burst size for rate limiting (e.g., 20)               |
| `evaluator`     | `poll_interval`        | `60`                                                | Seconds between git poll cycles                       |
| `evaluator`     | `git_timeout`          | `600`                                               | Git operation timeout (seconds)                       |
| `evaluator`     | `nix_timeout`          | `1800`                                              | Nix evaluation timeout (seconds)                      |
| `evaluator`     | `max_concurrent_evals` | `4`                                                 | Maximum concurrent evaluations                        |
| `evaluator`     | `work_dir`             | `/tmp/circus-evaluator`                             | Working directory for clones                          |
| `evaluator`     | `restrict_eval`        | `true`                                              | Pass `--option restrict-eval true` to Nix             |
| `evaluator`     | `allow_ifd`            | `false`                                             | Allow import-from-derivation                          |
| `queue_runner`  | `workers`              | `4`                                                 | Concurrent build slots                                |
| `queue_runner`  | `poll_interval`        | `5`                                                 | Seconds between build queue polls                     |
| `queue_runner`  | `build_timeout`        | `3600`                                              | Per-build timeout (seconds)                           |
| `queue_runner`  | `work_dir`             | `/tmp/circus-queue-runner`                          | Working directory for builds                          |
| `gc`            | `enabled`              | `true`                                              | Manage GC roots for build outputs                     |
| `gc`            | `gc_roots_dir`         | `/nix/var/nix/gcroots/per-user/circus/circus-roots` | GC roots directory                                    |
| `gc`            | `max_age_days`         | `30`                                                | Remove GC roots older than N days                     |
| `gc`            | `cleanup_interval`     | `3600`                                              | GC cleanup interval (seconds)                         |
| `logs`          | `log_dir`              | `/var/lib/circus/logs`                              | Build log storage directory                           |
| `logs`          | `compress`             | `false`                                             | Compress stored logs                                  |
| `cache`         | `enabled`              | `true`                                              | Serve a Nix binary cache at `/nix-cache/`             |
| `cache`         | `secret_key_file`      | none                                                | Signing key for binary cache                          |
| `signing`       | `enabled`              | `false`                                             | Sign build outputs                                    |
| `signing`       | `key_file`             | none                                                | Signing key file path                                 |
| `notifications` | `webhook_url`          | none                                                | HTTP endpoint to POST build status JSON               |
| `notifications` | `github_token`         | none                                                | GitHub token for commit status updates                |
| `notifications` | `gitea_url`            | none                                                | Gitea/Forgejo instance URL                            |
| `notifications` | `gitea_token`          | none                                                | Gitea/Forgejo API token                               |

<!--markdownlint-enable MD013 -->

## Database

Circus uses PostgreSQL with sqlx for compile-time query checking. Migrations
live in `crates/common/migrations/` and are added usually when the database
schema changes.

```bash
# Run pending migrations
$ cargo run --bin circus-migrate -- up <database_url>

# Validate schema
$ cargo run --bin circus-migrate -- validate <database_url>

# Create new migration file
$ cargo run --bin circus-migrate -- create <name>
```

Database tests gracefully skip when PostgreSQL is unavailable. To run the
database tests, make sure you build the test VMs.

## NixOS Deployment

Circus ships a NixOS module at `nixosModules.default`. Minimal configuration:

```nix
{
  inputs.circus.url = "github:notashelf/circus";

  outputs = { self, nixpkgs, circus, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        circus.nixosModules.default
        {
          services.circus = {
            enable = true;
            package = circus.packages.x86_64-linux.circus-server;
            migratePackage = circus.packages.x86_64-linux.circus-migrate-cli;

            server.enable = true;
            # evaluator.enable = true;
            # queueRunner.enable = true;
          };
        }
      ];
    };
  };
}
```

### Full Deployment Example

A complete production configuration with all three daemons and NGINX reverse
proxy:

```nix
{ config, pkgs, circus, ... }: {
  services.circus = {
    enable = true;
    package = circus.packages.x86_64-linux.circus-server;
    migratePackage = circus.packages.x86_64-linux.circus-migrate-cli;

    server.enable = true;
    evaluator.enable = true;
    queueRunner.enable = true;

    settings = {
      database.url = "postgresql:///circus?host=/run/postgresql";
      server.host = "127.0.0.1";
      server.port = 3000;

      # Security: enable when behind HTTPS reverse proxy
      server.force_secure_cookies = true;
      server.rate_limit_rps = 100;
      server.rate_limit_burst = 20;

      evaluator.poll_interval = 300;
      evaluator.restrict_eval = true;
      queue_runner.workers = 8;
      queue_runner.build_timeout = 7200;

      gc.enabled = true;
      gc.max_age_days = 90;
      cache.enabled = true;
      logs.log_dir = "/var/lib/circus/logs";
      logs.compress = true;
    };
  };

  # Reverse proxy
  services.nginx = {
    enable = true;
    virtualHosts."ci.example.org" = {
      forceSSL = true;
      enableACME = true;
      locations."/" = {
        proxyPass = "http://127.0.0.1:3000";
        proxyWebsockets = true;
        extraConfig = ''
          # FIXME: you might choose to harden this part further
          proxy_set_header X-Real-IP $remote_addr;
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
          proxy_set_header X-Forwarded-Proto $scheme;
          client_max_body_size 50M;
        '';
      };
    };
  };

  # Firewall
  networking.firewall.allowedTCPPorts = [ 80 443 ];
}
```

### Multi-Machine Deployment

For larger or _distributed_ setups, you may choose to run the daemons on
different machines sharing the same database. For example:

- **Head node**: runs `circus-server` and `circus-evaluator`, has the PostgreSQL
  database locally
- **Builder machines**: run `circus-queue-runner`, connect to the head node's
  database via `postgresql://circus@headnode/circus`

On builder machines, set `database.createLocally = false` and provide the remote
database URL:

```nix
{
  services.circus = {
    enable = true;
    database.createLocally = false; # <- Set this
    queueRunner.enable = true;

    # Now configure the database
    settings.database.url = "postgresql://circus@headnode.internal/circus";
    settings.queue_runner.workers = 16;
  };
}
```

Ensure the PostgreSQL server on the head node allows connections from builder
machines via `pg_hba.conf` (the NixOS `services.postgresql` module handles this
with `authentication` settings).

#### Remote Builders via SSH

Circus supports an alternative deployment model where a single queue-runner
dispatches builds to remote builder machines via SSH. In this setup:

- **Head node**: runs `circus-server`, `circus-evaluator`, and **one**
  `circus-queue-runner`
- **Builder machines**: standard NixOS machines with SSH access and Nix
  installed (no Circus software required)

The queue-runner automatically attempts remote builds using:

```bash
nix build --store ssh://<builder>
```

when a build's `system` matches a configured remote builder. If no remote
builder is available or all fail, it falls back to local execution.

You can configure remote builders via the REST API:

```bash
# Create a remote builder
curl -X POST http://localhost:3000/api/v1/admin/builders \
  -H 'Authorization: Bearer <admin-key>' \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "builder-1",
    "ssh_uri": "builder-1.example.org",
    "systems": ["x86_64-linux", "aarch64-linux"],
    "max_jobs": 4,
    "speed_factor": 1,
    "enabled": true
  }'
```

Do note that this requires some SSH key setup. Namely.

- The queue-runner machine needs SSH access to each builder (public key in
  `~/.ssh/authorized_keys` on builders)
- Use `ssh_key_file` in the builder config if using a non-default key
- Add known host keys via `public_host_key` to prevent MITM warnings

The queue-runner tracks builder health automatically: consecutive failures
disable the builder with exponential backoff until it recovers.

## Security

Circus implements multiple security layers to protect your CI infrastructure.
See [the security document](./SECURITY.md) for more details.

## Authentication

Circus supports two authentication methods:

1. **API Keys** - Bearer token authentication for API access
2. **User Accounts** - Username/password with session cookies for dashboard
   access

### API Key Bootstrapping

Circus uses SHA-256 hashed API keys stored in the `api_keys` table. To create
the first admin key after initial deployment:

```bash
# Generate a key and its hash
$ export CIRCUS_KEY="circus_$(openssl rand -hex 16)"
$ export CIRCUS_HASH=$(echo -n "$CIRCUS_KEY" | sha256sum | cut -d' ' -f1)

# Insert into the database
$ sudo -u circus psql -U circus -d circus -c \
  "INSERT INTO api_keys (name, key_hash, role) VALUES ('admin', '$CIRCUS_HASH', 'admin')"

# Save the key (it cannot be recovered from the hash)
$ echo "Admin API key: $CIRCUS_KEY"
```

Subsequent keys can be created via the API or the admin dashboard using this
initial admin key.

### User Management

Circus supports user accounts for dashboard access. Users can authenticate with
username/password and get a session cookie.

#### Creating Users

Users can be created via the API using an admin API key:

```bash
# Create a new user
curl -X POST http://localhost:3000/api/v1/users \
  -H 'Authorization: Bearer circus_demo_admin_key' \
  -H 'Content-Type: application/json' \
  -d '{
    "username": "developer",
    "email": "dev@example.com",
    "password": "secure-password-here",
    "role": "admin"
  }'
```

#### User Roles

Users can have roles that control their access level:

| Role        | Description                          |
| ----------- | ------------------------------------ |
| `admin`     | Full access to all endpoints         |
| `read-only` | Read-only access (GET requests only) |
| `custom`    | See role-based permissions below     |

Users inherit permissions from their role. The role can be set to any string for
custom permission schemes.

### Dashboard Login

The dashboard supports both authentication methods:

- **API Key Login**: Enter your API key on the login page for a session cookie
- **Username/Password**: Log in with user credentials for persistent sessions

Session cookies are valid for 24 hours and allow access to admin features
without re-entering credentials.

### Roles

> [!NOTE]
> Roles are an experimental feature designed to bring Circus on-par with
> enterprise-grade Hydra deployments. The feature is currently unstable, and
> might change at any given time. Do not rely on roles for the time being.

| Role              | Permissions                          |
| ----------------- | ------------------------------------ |
| `admin`           | Full access to all endpoints         |
| `read-only`       | Read-only access (GET requests only) |
| `create-projects` | Create projects and jobsets          |
| `eval-jobset`     | Trigger evaluations                  |
| `cancel-build`    | Cancel builds                        |
| `restart-jobs`    | Restart failed/completed builds      |
| `bump-to-front`   | Bump build priority                  |

## Monitoring

Circus exposes a Prometheus-compatible metrics endpoint at `/metrics`. The
metrics provided are detailed below:

### Available Metrics

| Metric                         | Type  | Description                 |
| ------------------------------ | ----- | --------------------------- |
| `circus_builds_total`          | gauge | Total number of builds      |
| `circus_builds_pending`        | gauge | Currently pending builds    |
| `circus_builds_running`        | gauge | Currently running builds    |
| `circus_builds_completed`      | gauge | Completed builds            |
| `circus_builds_failed`         | gauge | Failed builds               |
| `circus_projects_total`        | gauge | Total number of projects    |
| `circus_evaluations_total`     | gauge | Total number of evaluations |
| `circus_channels_total`        | gauge | Total number of channels    |
| `circus_remote_builders_total` | gauge | Configured remote builders  |

### Prometheus Configuration

```yaml
scrape_configs:
  - job_name: "circus-ci"
    static_configs:
      - targets: ["ci.example.org:3000"]
    metrics_path: "/metrics"
    scrape_interval: 30s
```

## Backup & Restore

Circus stores all state in PostgreSQL. To back up:

```bash
# Create a backup
$ pg_dump -U circus circus > circus-backup-$(date +%Y%m%d).sql
```

To restore:

```bash
# Restore a backup
$ psql -U circus circus < circus-backup-20250101.sql
```

Build logs are stored in the filesystem at the configured `logs.log_dir`
(default: `/var/lib/circus/logs`). You are generally encouraged to include this
directory in your backup strategy to ensure more seamless recoveries in the case
of a catastrophic failure. Build outputs live in the Nix store and are protected
by GC roots under `gc.gc_roots_dir`. These do not need separate backup as long
as derivation paths are retained in the database.

## API Overview

All API endpoints are under `/api/v1`. Write operations require a Bearer token
in the `Authorization` header. Read operations (GET) are public.

<!--markdownlint-disable MD013 -->

| Method | Endpoint                               | Auth                  | Description                        |
| ------ | -------------------------------------- | --------------------- | ---------------------------------- |
| GET    | `/health`                              | -                     | Health check with database status  |
| GET    | `/metrics`                             | -                     | Prometheus metrics                 |
| GET    | `/api/v1/projects`                     | -                     | List projects (paginated)          |
| POST   | `/api/v1/projects`                     | admin/create-projects | Create project                     |
| GET    | `/api/v1/projects/{id}`                | -                     | Get project details                |
| PUT    | `/api/v1/projects/{id}`                | admin                 | Update project                     |
| DELETE | `/api/v1/projects/{id}`                | admin                 | Delete project (cascades)          |
| GET    | `/api/v1/projects/{id}/jobsets`        | -                     | List project jobsets               |
| POST   | `/api/v1/projects/{id}/jobsets`        | admin/create-projects | Create jobset                      |
| GET    | `/api/v1/projects/{pid}/jobsets/{id}`  | -                     | Get jobset                         |
| PUT    | `/api/v1/projects/{pid}/jobsets/{id}`  | admin                 | Update jobset                      |
| DELETE | `/api/v1/projects/{pid}/jobsets/{id}`  | admin                 | Delete jobset                      |
| GET    | `/api/v1/evaluations`                  | -                     | List evaluations (filtered)        |
| GET    | `/api/v1/evaluations/{id}`             | -                     | Get evaluation details             |
| POST   | `/api/v1/evaluations/trigger`          | admin/eval-jobset     | Trigger evaluation                 |
| GET    | `/api/v1/builds`                       | -                     | List builds (filtered)             |
| GET    | `/api/v1/builds/{id}`                  | -                     | Get build details                  |
| POST   | `/api/v1/builds/{id}/cancel`           | admin/cancel-build    | Cancel build                       |
| POST   | `/api/v1/builds/{id}/restart`          | admin/restart-jobs    | Restart build                      |
| POST   | `/api/v1/builds/{id}/bump`             | admin/bump-to-front   | Bump priority                      |
| GET    | `/api/v1/builds/stats`                 | -                     | Build statistics                   |
| GET    | `/api/v1/builds/recent`                | -                     | Recent builds                      |
| GET    | `/api/v1/channels`                     | -                     | List channels                      |
| POST   | `/api/v1/channels`                     | admin                 | Create channel                     |
| DELETE | `/api/v1/channels/{id}`                | admin                 | Delete channel                     |
| POST   | `/api/v1/channels/{id}/promote/{eval}` | admin                 | Promote evaluation                 |
| GET    | `/api/v1/api-keys`                     | admin                 | List API keys                      |
| POST   | `/api/v1/api-keys`                     | admin                 | Create API key                     |
| DELETE | `/api/v1/api-keys/{id}`                | admin                 | Delete API key                     |
| GET    | `/api/v1/admin/builders`               | -                     | List remote builders               |
| POST   | `/api/v1/admin/builders`               | admin                 | Create remote builder              |
| PUT    | `/api/v1/admin/builders/{id}`          | admin                 | Update remote builder              |
| DELETE | `/api/v1/admin/builders/{id}`          | admin                 | Delete remote builder              |
| GET    | `/api/v1/admin/system`                 | admin                 | System status                      |
| GET    | `/api/v1/users`                        | admin                 | List users                         |
| POST   | `/api/v1/users`                        | admin                 | Create user                        |
| GET    | `/api/v1/users/{id}`                   | admin                 | Get user details                   |
| PUT    | `/api/v1/users/{id}`                   | admin                 | Update user                        |
| DELETE | `/api/v1/users/{id}`                   | admin                 | Delete user                        |
| GET    | `/api/v1/me`                           | user/api key          | Get current user                   |
| PUT    | `/api/v1/me`                           | user                  | Update current user                |
| POST   | `/api/v1/me/password`                  | user                  | Change password                    |
| GET    | `/api/v1/me/starred-jobs`              | user                  | List starred jobs                  |
| POST   | `/api/v1/me/starred-jobs`              | user                  | Star a job                         |
| DELETE | `/api/v1/me/starred-jobs/{id}`         | user                  | Unstar a job                       |
| GET    | `/api/v1/search?q=`                    | -                     | Search projects and builds         |
| GET    | `/api/v1/search/quick?q=`              | -                     | Quick search (backward compatible) |
| POST   | `/api/v1/webhooks/{pid}/github`        | HMAC                  | GitHub webhook                     |
| POST   | `/api/v1/webhooks/{pid}/gitea`         | HMAC                  | Gitea/Forgejo webhook              |
| GET    | `/nix-cache/nix-cache-info`            | -                     | Binary cache info                  |
| GET    | `/nix-cache/{hash}.narinfo`            | -                     | NAR info lookup                    |
| GET    | `/nix-cache/nar/{hash}.nar.zst`        | -                     | NAR download (zstd)                |

<!--markdownlint-enable MD013 -->

### Dashboard

The web dashboard is available at the root URL (`/`). Pages include:

- `/` - Home: build stats, project overview, recent builds and evaluations
- `/projects` - Project listing with create form (admin)
- `/project/{id}` - Project detail with jobsets, add jobset form (admin)
- `/evaluations` - Evaluation listing with project/jobset context
- `/builds` - Build listing with status/system/job filters
- `/queue` - Current queue (pending + running builds)
- `/channels` - Channel listing
- `/admin` - System status, API key management, remote builder management, user
  management
- `/login` - Session-based login supporting both username/password and API key
  authentication
- `/logout` - Log out and clear session

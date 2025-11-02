# FC

FC is a modern, Rust-based continuous integration system designed to replace
Hydra for our systems. Heavily work in progress.

## Architecture

- **server**: Web API and UI (Axum)
- **evaluator**: Git polling and Nix evaluation
- **queue-runner**: Build dispatch and execution
- **common**: Shared types and utilities

## Development

```bash
nix develop
cargo build
```

## Components

### Server

Web API server providing REST endpoints for project management, jobsets, and
build status.

### Evaluator

Periodically polls Git repositories and evaluates Nix expressions to create
builds.

### Queue Runner

Processes build queue and executes Nix builds on available machines.

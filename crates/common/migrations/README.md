# Database Migrations

This directory contains SQL migrations for the FC database.

## Migration Files

- `001_initial_schema.sql`: Creates the core database schema including projects,
  jobsets, evaluations, builds, and related tables.

## Running Migrations

The easiest way to run migrations is to use the vendored CLI, `fc-migrate`.
Packagers should vendor this crate if possible.

```bash
# Run all pending migrations
fc-migrate up postgresql://user:password@localhost/fc_ci

# Validate current schema
fc-migrate validate postgresql://user:password@localhost/fc_ci

# Create a new migration
fc-migrate create migration_name
```

TODO: add or generate schema overviews

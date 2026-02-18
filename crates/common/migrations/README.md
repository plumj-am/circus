# Database Migrations

This directory contains SQL migrations for the FC database.

## Migration Files

- `0001_schema.sql`: Full schema, all tables, indexes, triggers, and views.
- `0002_example.sql`: Example stub for the next migration when we make a stable
  release.

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

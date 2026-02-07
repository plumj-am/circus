<!--markdownlint-disable MD013-->

# FC vs Hydra

FC is not exactly a competitor, but a direct replacement to Hydra with a limited
set of features that are prioritized. While FC hopes to be a complete
replacement, not everything can be considered in-scope for a team of FOSS
developers without funding. _Should_ you create any issues, your feature may or
may not be implemented depending on our own needs. Please keep that in mind.

Below document contains a "feature matrix" of concepts and features that we've
decided to think about. Not all of them will be fully implemented (i.e., we are
a-okay with being less powerful in some regards) but FC _does_ aim to be better
than Hydra in the long term, through different means; reliability, UX and
performance.

## Executive Summary

FC currently implements more or less 50% of Hydra's core features, and has
various improvements over Hydra's architecture. As of writing, some gaps (such
as the plugin architecture, VCS diversity and notification integrations) remain.

### Strengths

- Modern Rust codebase with better error handling
- Simpler, more maintainable architecture (5 Rust crates vs Perl & C++ monolith)
- Better API-first design with proper REST structure
- User management with argon2 password hashing and granular RBAC
- Cleaner database schema (16 tables vs 70+)
- Better Nix Flake support from day one
- Improved & tested declarative jobsets

### Weaknesses

- Limited VCS support (Git only in FC vs 6 types in Hydra)
- No plugin architecture for extensibility
- Missing several notification integrations (Slack, BitBucket, etc.)
- No declarative project specification (coming soon)
- No coverage/build metrics collection

TODO: add a better comparison matrix

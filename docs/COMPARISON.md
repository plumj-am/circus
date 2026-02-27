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

FC currently implements _more or less_ 50% of Hydra's core features, and has
various improvements over Hydra's architecture. As of writing, some gaps (such
as the plugin architecture, VCS diversity and notification integrations) remain.

As FC is currently in _heavy_ development, those gaps will remain for the
foreseeable future, however, most _critical_ functionality has already been
implemented. In any case, I believe FC has made good progress on the path of
being a "better Hydra".

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

## Feature-by-Feature

### FC Server

`fc-server` crate is the REST API server that powers FC. In comparison to
support for full CRUD operations (on par with Hydra), FC exceeds Hydra in
several areas, such as log streaming, evaluation comparison, build actions or
metrics visualization from the API. Below is a comparison table for the sake of
historical documentation and progress tracking:

| Feature                  | Hydra            | FC                  | Status   | Notes                              |
| ------------------------ | ---------------- | ------------------- | -------- | ---------------------------------- |
| **REST API Structure**   | OpenAPI 3.0 spec | REST                | Complete | FC has cleaner `/api/v1` structure |
| **Project Endpoints**    | Full CRUD        | Full CRUD           | Complete |                                    |
| **Jobset Endpoints**     | Full CRUD        | Full CRUD           | Complete | FC has jobset inputs               |
| **Build Endpoints**      | Full             | Full + actions      | Complete | FC has cancel/restart/bump         |
| **Evaluation Endpoints** | Basic            | Full + trigger      | Complete | FC has trigger + compare           |
| **Search API**           | Full search      | Advanced search     | Complete | Multi-entity, filters, sorting     |
| **Channel API**          | Management       | Full CRUD           | Complete |                                    |
| **User API**             | User management  | Full CRUD + auth    | Complete |                                    |
| **Binary Cache API**     | NAR/manifest     | Full cache protocol | Complete | e                                  |
| **Webhook API**          | Push trigger     | GitHub/Gitea        | Complete | FC has HMAC verification           |
| **Badge API**            | Status badges    | Implemented         | Complete | Both support badges                |
| **Metrics API**          | Prometheus       | Prometheus          | Complete | Both expose metrics                |
| **Log Streaming**        | Polling only     | SSE streaming       | Complete | FC has Server-Sent Events          |

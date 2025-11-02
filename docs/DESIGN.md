# Design

Notes to self and somewhat of a guide to design some of the design choices
behind FC. This is not a contribution guideline, and changes to this document
are welcome if necessary.

## Overview

FC is built as a local replacement for Hydra. Meaning you probably do not want
to deploy it on your Super Enterprise Friends Group that needs a reliable CI.
This project is an attempt to utilize our infrastructure members as build
machines to cache our projects without relying on Github's weak runners.

---

Hydra is the Nix/NixOS project's continuous integration system. It uses Nix to
declaratively define and build jobs, ensuring reproducible builds. According to
the NixOS Wiki:

> "Hydra is a tool for continuous integration testing and software release that
> uses a purely functional language to describe build jobs and their
> dependencies."

In Hydra:

- A **Project** corresponds to a source repository.
- A **Jobset** (often per branch or channel) contains many **Jobs** (Nix
  derivations to build).
- A `release.nix` ("Release Set") file declares what to build.

Hydra pulls changes from version control, re-evaluates Nix expressions, and
triggers builds when inputs change.

---

FC commits to this design with minimal tweaks. Most critically, FC is not
designed to be used alongside Nixpkgs. Sure you can do it, but that is not the
main goal. The main goal is a distributed, declarative CI that has learned from
Hydra's mistakes.

### Component Interactions and Data Flow

Hydra follows a tightly-coupled architecture with three main daemons:

```ascii
Git Repository -> Evaluator -> Database -> Queue Runner -> Build Hosts -> Results -> Database -> Web UI
```

In this flow, the responsible components are as follows:

1. **hydra-server** (Perl, Catalyst): Web interface and REST API
2. **hydra-evaluator**: Polls Git repos, evaluates Nix expressions, creates
   `.drv` files
3. **hydra-queue-runner**: Dispatches builds to available builders via SSH/Nix
   remote
4. **Database (PostgreSQL)**: Central state management for all components

While simple on paper, this design leads to several issues. Besides the single
point of failure (the database), the tight coupling leads to requiring shared
database state and contributes to the lack of horizontal scaling in Hydra. Also
worth nothing that the evaluator must complete before the queue runner can
dispatch, the dependencies are synchronous.

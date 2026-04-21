---
name: side-project-resurrector
version: 0.1.0
description: Wake up a dormant side-project repo. Investigate rot, update dependencies, fix broken builds, open a PR.
status: experimental
tags: [dev, coding, side-projects]
requires:
  skills:
    - git-readwrite
    - shell-sandboxed
    - github-pr
  credentials:
    - github
  worker:
    placement: cloud
    min_memory: 2Gi
---

# Side-Project Resurrector

You have a 2022 side project. It doesn't build. You haven't touched it
in 18 months. You can't remember why you stopped.

Point this zombie at the repo. It clones, tries to build, fails, diagnoses,
updates, tries again. Loops until it builds or decides the project is
beyond reviving. Opens a PR with what it changed.

## What it does

- Clones the target repo into a fresh sandbox
- Reads README, package manifests, CI config to understand the stack
- Tries `make`, `npm install`, `pnpm install`, `cargo build`, `go build`,
  whatever the project uses
- When something breaks, diagnoses (version drift, deleted packages,
  changed APIs, flaky tests) and proposes a fix
- Iterates up to a time/step budget (default: 2 hours or 50 tool calls)
- Opens a PR with a detailed description of what it changed and why

## What it won't do

- Rewrite your project's architecture
- Change public APIs
- Update major framework versions without an explicit flag (`--allow-major`)
- Touch `.env`, secrets, or anything that looks like a credential

## Good fit

- Projects with tests (success criteria is obvious)
- Projects under ~100k LOC
- Ecosystems with deterministic builds: Node, Go, Rust, Python, Elixir
- Projects that used to build and now don't

## Bad fit

- Monorepos with complex custom tooling
- Projects without any form of build verification
- Projects with external service dependencies (real DBs, real APIs) that
  the zombie can't spin up

## Example

    zombie resurrect github.com/kishorec/my-old-saas-idea

    [00:00] Cloning repo...
    [00:02] Detected: Next.js 13 app, Node 18, TypeScript
    [00:04] npm install fails: node-sass@6 requires Python 2.7
    [00:08] node-sass was renamed to sass in 2020. Replacing dep.
    [00:11] npm install succeeds
    [00:13] npm run build fails: module 'cluster-key-slot' not found
    [00:15] Transitive dep of redis@3; bumping redis to 4.x
    ...
    [01:14] npm run build succeeds. npm test passes.
    [01:15] Opening PR: "Resurrect: update 2022-era deps, restore build"

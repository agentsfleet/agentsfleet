---
name: docker-readonly
version: 0.1.0
description: Read-only Docker access to a host or set of hosts. Diagnostics only.
tags: [docker, diagnostics, readonly]
credentials:
  - type: ssh
    description: SSH key to the Docker host (or local socket on the worker)
policy:
  allowed_commands: [ps, logs, inspect, images, stats, top, events, version, info]
  auto_approve: true
---

# docker-readonly

A read-only Docker skill for diagnosing container issues on homelab hosts.

## What the agent can do

- `docker ps [-a]` — list containers
- `docker logs <container>` — with tail limits
- `docker inspect <container|image|network|volume>`
- `docker images`, `docker stats`, `docker top`, `docker events`
- `docker version`, `docker info`

## What the agent cannot do

- No `run`, `exec`, `start`, `stop`, `restart`, `rm`, `rmi`, `pull`, `push`,
  `build`, `tag`, `commit`, `kill`, `pause`, `unpause`, `update`
- No compose commands (write-capable) — compose support is planned behind
  a separate write-capable skill with approval gates
- No access to host filesystem via bind mounts or volumes

## Implementation

The skill shells out to the `docker` CLI on the target host, either via
the local Docker socket (if the worker runs on the Docker host) or via
SSH to a remote host. All commands are parsed and verified against the
allowlist before execution.

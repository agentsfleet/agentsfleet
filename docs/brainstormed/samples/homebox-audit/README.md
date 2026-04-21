---
name: homebox-audit
version: 0.1.0
description: Audit your self-hosted stack — outdated containers, insecure configs, expired TLS, weak secrets.
status: experimental
tags: [ops, selfhosted, homelab, security]
requires:
  skills:
    - docker-readonly
    - kubectl-readonly
    - tls-probe
  credentials:
    - kube
    - ssh
  worker:
    placement: customer-network
---

# Homebox Audit

A quarterly audit for your homelab, run by a zombie. It checks:

- Outdated containers (image age, known CVEs)
- TLS certificates nearing expiry
- Default passwords, anonymous access, unprotected dashboards
- Unneeded ports exposed to the public internet
- Missing backups (inferred from volume inspection)

Produces a prioritized report. Shareable with your future self.

## Good reason to run it

- You forgot when you last updated Home Assistant
- You have no idea whether your Jellyfin is on the public internet
- Your Let's Encrypt cert renewed… or didn't
- You want a monthly "state of the homelab" report to your own email

## What it won't do

- Update anything
- Rotate secrets
- Change configs

v0.2 adds proposed remediations with approval gates.

## Example

    zombie
    → Homebox audit ready. Run the full sweep? [Y/n]
    > y

    [00:00] Enumerating containers across 3 Docker hosts + 1 k3s cluster
    [00:08] 47 containers, 12 deployments, 8 stateful apps
    [00:15] Checking image ages...
            → 14 containers on images > 6 months old
            → 3 containers on images > 12 months old (jellyfin, immich, paperless)
    [00:22] Probing TLS on public endpoints...
            → 6 certs valid, 1 expiring in 9 days (nextcloud.home.example)
    [00:31] Scanning for default credentials on known services...
            → grafana has default admin/admin
    [00:40] Audit complete.

    --- Homebox audit report (2026-04-20) ---

    Critical (fix this week):
      1. nextcloud.home.example TLS cert expires in 9 days
      2. grafana has default admin/admin credentials

    High (fix this month):
      3. jellyfin on image released 2023-11-02 (14 months old)
         → 4 known CVEs, 1 high severity
      4. immich image is 10 months stale

    ...

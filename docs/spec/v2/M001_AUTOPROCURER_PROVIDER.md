# M001: Spec — Autoprocurer Multi-Provider Design

**Version:** v2
**Updated:** Mar 21, 2026
**Status:** DESIGN — not yet implemented. Current execution uses OVHCloud manually (M4_001).

The autoprocurer must be able to provision bare-metal servers from any approved provider without hardcoding provider-specific logic into the pipeline. This spec defines the provider abstraction, the provider playbook format, and the interface the autoprocurer consumes.

---

## Problem

Today (M4_001), the human buys a server from OVHCloud. That step is the only remaining human bottleneck after the initial account setup. The autoprocurer agent is designed to eliminate it — but only if it can work against a stable provider interface, not a specific vendor API.

Providers differ in:
- API authentication (API key, OAuth, service account)
- Ordering model (on-demand, reserved, spot)
- OS install mechanism (reinstall API, PXE, netboot)
- Credential delivery (root password email, SSH key injection, console)
- Provisioning time (minutes to hours)

The abstraction must hide these differences from the rest of the pipeline.

---

## Provider Interface

Every provider plugin exposes four operations:

```
provision(spec) → server_handle
  spec: { region, machine_type, os, hostname }
  returns: { provider_id, ip, credential_type, credential_value, estimated_ready_at }

status(server_handle) → { state: provisioning | ready | error, ip, message }

destroy(server_handle) → ok

list_machine_types(region) → [{ id, cpu, ram_gb, disk_gb, has_kvm, monthly_usd }]
```

The autoprocurer calls `provision` → polls `status` until `ready` → hands off `{ ip, credential_type, credential_value }` to **autoprovisioner**.

The autoprovisioner does not know or care which provider was used.

---

## Provider Playbook Format

The provider playbook is a versioned YAML file (`providers/playbook.yml`) listing approved providers. The autoprocurer reads this file; it never hardcodes a provider.

```yaml
# providers/playbook.yml
version: 1
providers:

  - id: ovhcloud
    name: OVHCloud Bare-Metal
    status: active           # active | deprecated | evaluation
    regions:
      - id: bhs              # Beauharnois, Canada
        display: Beauharnois CA
      - id: gra              # Gravelines, France
        display: Gravelines FR
    machine_types:
      - id: ks-1
        has_kvm: true
        monthly_usd: 45
        notes: "Dev/test — Firecracker compatible"
      - id: ks-b
        has_kvm: true
        monthly_usd: 95
        notes: "Prod — higher RAM"
    credential_delivery: root_password_email
    plugin: plugins/ovhcloud.py
    vault_item: ovhcloud-api          # ZMB_CD_PROD/ovhcloud-api/{app-key,app-secret,consumer-key}
    requires_kvm: true                # only select machine types where has_kvm: true

  # Future entries added by autoreviewer:
  # - id: hetzner
  # - id: vultr
  # - id: equinix-metal
```

---

## Autoprocurer Selection Logic

When the autoforecaster emits a scaling recommendation, the autoprocurer selects a provider and machine type using:

1. **KVM required** — filter to `has_kvm: true` entries (Firecracker requirement)
2. **Region affinity** — prefer the region closest to the data plane (Upstash, PlanetScale endpoints)
3. **Status: active** — skip `deprecated` or `evaluation` entries
4. **Cost** — select the lowest `monthly_usd` that meets the spec
5. **Availability** — if `provision()` returns an error, try the next provider in rank order

The selection is deterministic and logged — the autoprocurer records which provider was chosen and why in the vault item it creates.

---

## Autoreviewer

The **autoreviewer** agent runs periodically (weekly) and:

1. Fetches pricing and availability data from candidate providers
2. Evaluates against criteria: KVM support, bare-metal availability in target regions, pricing vs current, compliance (data residency, SOC2)
3. Proposes changes to `providers/playbook.yml` as a PR for human review
4. Does NOT auto-merge — a human approves provider additions

This keeps the playbook current without manual research while keeping a human in the loop for provider trust decisions.

---

## Vault Contract

The autoprocurer creates a 1Password item for each provisioned server before handing off to autoprovisioner:

```
vault:    ZMB_CD_PROD (or ZMB_CD_DEV for dev nodes)
item:     zombie-{env}-worker-{animal}   e.g. zombie-prod-worker-ant
fields:
  ssh-private-key    [concealed]   deploy key (written by autoprovisioner §1.0)
  hostname           zombie-prod-worker-ant
  tailscale-ip       100.x.x.x    (written after §3.0)
  provider           ovhcloud
  provider-server-id <ovhcloud server ID>
  region             bhs
  provisioned-at     2026-03-21T04:00:00Z
```

Every downstream agent (autohardener, autoworkerstandup, autoupgrader) reads from this vault item. The provider identity is recorded but never used after provisioning.

---

## Current State → Future State

| Today (M4_001) | Future (this spec) |
|---|---|
| Human orders from OVHCloud console | autoprocurer calls `ovhcloud.provision()` |
| Human records IP + root password | autoprocurer writes `{ ip, credential }` to vault |
| Human hands off to agent | autoprovisioner picks up from vault automatically |
| OVHCloud only | Any provider in `providers/playbook.yml` |
| Manual provider selection | Automated selection by cost + KVM + region |

The M4_001 playbook is the manual implementation of this spec. When the autoprocurer is built, step 0.0 (Human: Buy server) is eliminated.

---

## Implementation Notes

- Provider plugins live in `providers/plugins/<id>.py` (or equivalent language)
- Each plugin is tested against a sandbox/staging account before `status: active`
- The autoprocurer records its decision log in the server's vault item for auditability
- `destroy()` is called by autodrainer — the same plugin handles teardown

# Auto-Agent Pipeline — Worker Scaling

High-level pipeline for agent-managed infrastructure. Each agent will be built as a usezombie spec.

---

## Design Principle: Agent-First Sequencing

Steps are ordered to maximize what the agent can do autonomously after the minimum human handoff. The human hands off one artifact (a server with an IP and initial credentials); the agent takes it from there without further human interaction.

The agent-first sequence matters because:
- Human steps are bottlenecks — minimize them and front-load them
- Every step after the human handoff must be retryable and idempotent
- Vault is the handoff contract between steps — each step reads what the previous step wrote

See `docs/M4_001_PLAYBOOK_WORKER_BOOTSTRAP_DEV.md` for the concrete implementation of this pattern.

---

## Pipeline

```
demand signal → autoforecaster → autoprocurer → autoprovisioner → autohardener
  → autovaultsaver → autoinfraready → autoworkerstandup → autoworkerready

failure signal → autodrainer → (autoprocurer if replacement needed)

periodic → autoupgrader
```

---

## Agents

| # | Agent | Trigger | Input | Output |
|---|---|---|---|---|
| 1 | **autoforecaster** | queue depth, run duration trends, time-of-day | Grafana metrics | scaling recommendation (add/remove N workers) |
| 2 | **autoprocurer** | scaling recommendation | provider playbook | server provisioned, IP + initial credentials |
| 3 | **autoprovisioner** | new server | IP + initial credentials | deploy SSH key in vault, Tailscale joined, public IP access dropped |
| 4 | **autohardener** | provisioned server | SSH via Tailscale | firewall, fail2ban, Debian security baseline, KVM verified |
| 5 | **autovaultsaver** | hardened server | server name + credentials | 1Password item created (`zombie-{env}-worker-{name}`, e.g. `zombie-prod-worker-ant`) |
| 6 | **autoinfraready** | vaulted server | server record | Discord notification, server marked ready |
| 7 | **autoworkerstandup** | ready server + latest release tag | server + binary | worker process running, joined Redis consumer group |
| 8 | **autoworkerready** | deployed worker | health check | verified healthy, serving runs |
| 9 | **autodrainer** | failure signal (Grafana alert) | failing server | runs redistributed via XAUTOCLAIM, server drained |
| 10 | **autoupgrader** | new release tag (periodic) | all workers | rolling upgrade, zero-downtime |

---

## Provider Playbook

The **autoprocurer** reads from a versioned provider playbook — it does not hardcode a provider. See `docs/spec/v2/M001_AUTOPROCURER_PROVIDER.md` for the full multi-provider spec.

Current: OVHCloud (Beauharnois CA, bare-metal) — the only active entry today. An **autoreviewer** agent periodically evaluates new providers for cost, availability, and compliance, then updates the playbook.

Worker naming: alphabetical animals, prefixed by environment (`zombie-dev-worker-ant`, `zombie-prod-worker-ant`, `zombie-prod-worker-bird`, ...).

---

## Observability Loop

Grafana Cloud feeds signals to autoforecaster and autodrainer:
- Queue depth > threshold → scale up
- Worker health check failure → drain + replace
- Run duration trending up → investigate capacity
- Idle workers for sustained period → scale down

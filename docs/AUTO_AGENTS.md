# Auto-Agent Pipeline — Worker Scaling

High-level pipeline for agent-managed infrastructure. Each agent will be built as a usezombie spec.

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
| 2 | **autoprocurer** | scaling recommendation | provider playbook | server provisioned, IP + credentials |
| 3 | **autoprovisioner** | new server | IP + credentials | OS installed, Tailscale-joined |
| 4 | **autohardener** | provisioned server | SSH access | firewall, fail2ban, Debian Trixie security baseline |
| 5 | **autovaultsaver** | hardened server | server name + credentials | 1Password item created (`zombie-worker-{name}`) |
| 6 | **autoinfraready** | vaulted server | server record | Discord notification, server marked ready |
| 7 | **autoworkerstandup** | ready server + latest release tag | server + binary | worker process running, joined Redis consumer group |
| 8 | **autoworkerready** | deployed worker | health check | verified healthy, serving runs |
| 9 | **autodrainer** | failure signal (Grafana alert) | failing server | runs redistributed via XAUTOCLAIM, server drained |
| 10 | **autoupgrader** | new release tag (periodic) | all workers | rolling upgrade, zero-downtime |

---

## Provider Playbook

Current: OVHCloud (Beauharnois CA, bare-metal).

The autoprocurer reads from a provider playbook — a versioned list of approved providers with API integrations. An **autoreviewer** agent periodically evaluates new providers for cost, availability, and compliance, then updates the playbook for the autoprocurer to access.

Worker naming: alphabetical animals (`zombie-worker-ant`, `zombie-worker-bird`, ...).

---

## Observability Loop

Grafana Cloud feeds signals to autoforecaster and autodrainer:
- Queue depth > threshold → scale up
- Worker health check failure → drain + replace
- Run duration trending up → investigate capacity
- Idle workers for sustained period → scale down

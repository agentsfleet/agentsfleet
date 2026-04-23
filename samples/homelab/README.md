# Homelab Zombie

An AI agent that diagnoses problems in a homelab — small Kubernetes
cluster plus a Docker host — and never holds the cluster credentials.
Ask it "Jellyfin pods keep restarting" and it reads pod state, logs,
and events through read-only kubectl and docker tools, reasons about
what it sees, and returns a diagnosis.

This sample is the flagship executable zombie for v2.0-alpha. It is
read-only by design; remediation is a separate zombie behind approval
gates (not included here).

## Prerequisites

- `zombied` running locally or reachable via the `zombiectl` config.
  If you're running it locally, start the daemon with `zombied serve`
  (the zombied-api process, see `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md §1`).
- A Clerk-authed tenant / workspace (follow the quickstart if you
  haven't done this yet).
- `kubectl` binary on the worker image. The default dev worker image
  includes it; custom images need `kubectl >= 1.28`.
- Kubeconfig for the target cluster, available locally as
  `~/.kube/config` (or wherever you keep it).
- Docker Engine on the host where the worker runs (for the docker
  tool). Optional — if you only want the kubectl side, skip the
  docker credential and the zombie will reason from cluster evidence
  alone.

## Step 1 — Add credentials to the vault

The zombie never sees raw credentials. The vault holds them; the
worker injects them at the network boundary when a tool call runs.

```bash
# kubeconfig: byte contents are read from the file and stored encrypted.
zombiectl credential add kubectl_config --file ~/.kube/config

# docker socket: the PATH is stored as a connection hint, not the
# socket's byte contents (a Unix socket is not a readable file).
# --path is the right flag here; --file on /var/run/docker.sock would
# fail or store nothing. For a remote Docker engine, pass a TCP URL
# (tcp://host:2376) via --path instead, plus separate --ca-cert +
# --client-cert credentials.
zombiectl credential add docker_socket --path /var/run/docker.sock
```

If you don't have a Docker host to point at, skip the second line.
The zombie will still run; it will just reason from cluster evidence
only.

## Step 2 — Install the zombie

From the root of the `usezombie` checkout:

```bash
zombiectl install --from samples/homelab
```

Expected output: `🎉 homelab is live.` and a zombie ID. The zombie is
registered and active the moment this command returns (no separate
start step — see `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md §8 invariant #1`).
Fetch the webhook URL with `zombiectl status` when you need it for
external wiring.

## Step 3 — Trigger it

Grab the webhook URL for the zombie you just installed:

```bash
WEBHOOK_URL=$(zombiectl status --json | jq -r '.zombies[] | select(.name=="homelab") | .webhook_url')
```

Then POST an event:

```bash
curl -X POST \
  -H 'Content-Type: application/json' \
  -d '{"message":"Jellyfin pods keep restarting"}' \
  "$WEBHOOK_URL"
```

Watch the activity stream. You should see:

1. `zombie_triggered` — the webhook arrived.
2. Several `tool_call_requested` / `tool_call_completed` pairs as the
   zombie runs `kubectl get pods`, `kubectl describe pod ...`,
   `kubectl logs ...`, possibly `kubectl top pod ...`.
3. A final `zombie_completed` event carrying a short diagnosis.

## Example conversation

**You ask:** `Jellyfin pods keep restarting`

**The zombie runs (roughly):**

```
kubectl get pods -n media
  → jellyfin-7f9c-xxxxx  CrashLoopBackOff  3 restarts in 5m
kubectl describe pod jellyfin-7f9c-xxxxx -n media
  → Last State: Terminated, Reason: OOMKilled, Exit Code: 137
kubectl top pod -n media
  → jellyfin 756Mi (limit 512Mi)
kubectl logs jellyfin-7f9c-xxxxx -n media --previous --tail=50
  → "OutOfMemoryError during library scan at 14:32:05 UTC"
```

**The zombie concludes:**

> Diagnosis: Jellyfin pods are OOMKilled. The deployment's memory
> limit (512Mi) is below the observed working set (~780Mi) during the
> library scan.
>
> Suggested next step: raise memory limit in the jellyfin Deployment
> to 1Gi and re-apply. Requires a separate write-enabled zombie or
> manual kubectl.

Total tool calls: 4. Total time: about 8–12 seconds, most of it
waiting on the cluster API.

## What the zombie may and may not do

The allowlist lives as prose in `SKILL.md`. Read the **Tools you can
use** section there for the authoritative list. Short version:

- **kubectl**: `get`, `describe`, `logs`, `top`, `events`, `explain`,
  `version`, `api-resources`, `api-versions`. No destructive verbs.
  No reads of `secrets` resources.
- **docker**: `ps`, `logs`, `inspect`, `images`, `stats`, `top`,
  `events`, `version`, `info`. No mutating commands, no pulls, no
  builds.

When the tool dispatcher runs (nullclaw, when it ships) it will
enforce the prose allowlist — a rejected call surfaces as a
structured error the agent can reason from.

## Missing credential? Clean halt.

If you trigger the zombie before adding `kubectl_config` to the
vault, the first kubectl tool call will emit a single
`UZ-GRANT-001` event pointing at the fix:

```
UZ-GRANT-001: credential 'kubectl_config' not found in vault.
  Run: zombiectl credential add kubectl_config --file ~/.kube/config
```

The zombie stops cleanly — no crash, no partial writes, no retries
against a cluster it can't reach.

## How it works (two paragraphs)

**Credentials never leave the worker.** When you add a kubeconfig via
`zombiectl credential add`, it goes into the tenant vault encrypted.
When the zombie invokes a kubectl tool call, the worker parses the
command, verifies it against the allowlist described in `SKILL.md`,
and injects the credential at the HTTPS boundary to the cluster API
server. The agent itself only sees command output — never the
kubeconfig bytes, bearer token, or certificate material. If the
model is prompt-injected into asking for credentials, the worst it
can leak is an opaque placeholder identifier; there is no real
token reachable from the agent's context.

**Policy is prose, not YAML.** The "what's allowed" rules for
kubectl and docker live as natural language in `SKILL.md` — no
separate policy blocks, no sub-skill files. The LLM reads the prose
as part of its instructions and the dispatcher enforces the same
allowlist at tool-call time. For this single-consumer sample, one
file per zombie is enough; if a second zombie ever wants to share
the same allowlist, we'll lift it then.

## Limitations (MVP)

- Single kubectl context. Multi-cluster is a future milestone.
- Docker Engine only. No Docker Swarm, no Compose, no Podman for MVP.
- No Slack or chat-driven invocation — webhook is the only trigger
  shipped here.
- Read-only. Remediation (e.g. raising a memory limit) is a separate
  zombie that sits behind an approval gate — not included in this
  sample.

## Related

- `docs/brainstormed/docs/homelab-zombie-launch.md` — the launch-post
  narrative this sample implements.

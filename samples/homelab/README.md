# Homelab Zombie

An AI agent that diagnoses problems in a homelab — small Kubernetes
cluster plus a Docker host — and never holds the cluster credentials.
Ask it "Jellyfin pods keep restarting" and it reads pod state, logs,
and events through a read-only tool layer, reasons about what it
sees, and returns a diagnosis.

This sample is the flagship executable zombie for v2.0-alpha. It is
read-only by design; remediation is a separate zombie behind approval
gates (not included here).

## Prerequisites

- `zombied` running locally or reachable via the `zombiectl` config
  (`zombiectl up` if running locally).
- A Clerk-authed tenant / workspace (follow the quickstart if you
  haven't done this yet).
- `kubectl` binary on the worker image. The default dev worker image
  includes it; custom images need `kubectl >= 1.28`.
- Kubeconfig for the target cluster, available locally as
  `~/.kube/config` (or wherever you keep it).
- Docker Engine on the host where the worker runs (for the
  `docker-readonly` sub-skill). Optional — if you only want the
  kubectl side, skip the docker credential and the zombie will
  reason from cluster evidence alone.

## Step 1 — Add credentials to the vault

The zombie never sees raw credentials. The vault holds them; the
worker injects them at the network boundary when a tool call runs.

```bash
zombiectl credential add kubectl_config --file ~/.kube/config
zombiectl credential add docker_socket --file /var/run/docker.sock
```

If you don't have a Docker host to point at, skip the second line.
The zombie will still run; it will just reason from cluster evidence
only.

## Step 2 — Install the zombie

From the root of the `usezombie` checkout:

```bash
zombiectl zombie install --from samples/homelab
```

Expected output: a zombie ID and a webhook URL to trigger it.

The install reads `SKILL.md` and `TRIGGER.md` from this directory,
validates the sub-skill policies under `skills/`, registers the
zombie on your tenant, and prints the webhook URL.

## Step 3 — Trigger it

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

## Firewall allowlist

The two sub-skills declare the exact verbs/commands the zombie is
permitted to run. Attempts to use anything else are rejected by the
tool dispatcher before the command touches the cluster.

**`kubectl-readonly`** (see `skills/kubectl-readonly/SKILL.md`):
- Verbs: `get`, `describe`, `logs`, `top`, `events`, `explain`,
  `version`, `api-resources`, `api-versions`.
- Resources: all except `secrets` (denied even for `get`).
- Destructive verbs (`delete`, `apply`, `patch`, `exec`, `scale`,
  `rollout`, `cordon`, `drain`) are rejected.

**`docker-readonly`** (see `skills/docker-readonly/SKILL.md`):
- Commands: `ps`, `logs`, `inspect`, `images`, `stats`, `top`,
  `events`, `version`, `info`.
- Mutating commands (`run`, `exec`, `start`, `stop`, `rm`, `kill`,
  `build`, `push`, `pull`, `pause`) are rejected.

When a verb is rejected, the zombie receives a structured error
string it can reason from and try an allowed verb instead. The
rejection is visible in the activity stream as
`firewall_request_blocked` / `UZ-FIREWALL-001`.

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
When the zombie invokes a `kubectl-readonly` tool call, the worker
parses the command, verifies the verb against the allowlist, and
injects the credential at the HTTPS boundary to the cluster API
server. The agent itself only sees command output — never the
kubeconfig bytes, bearer token, or certificate material. If the model
is prompt-injected into asking for credentials, the worst it can leak
is an opaque placeholder identifier; there is no real token reachable
from the agent's context.

**Tools are verb-allowlisted, not sandboxed.** The sub-skills declare
which verbs (`kubectl-readonly/SKILL.md`) and commands
(`docker-readonly/SKILL.md`) are allowed. The dispatcher checks every
tool call against that allowlist before execution. Verbs not on the
list fail closed — the agent sees an error, reasons from it, and
tries something allowed. This is cheaper and more auditable than
running the whole worker inside a tight sandbox, and it makes the
policy human-readable in one file per skill.

## Limitations (MVP)

- Single kubectl context. Multi-cluster is a future milestone.
- Docker Engine only. No Docker Swarm, no Compose, no Podman for MVP.
- No Slack or chat-driven invocation — webhook is the only trigger
  shipped here.
- Read-only. Remediation (e.g. raising a memory limit) is a separate
  zombie that sits behind an approval gate — not included in this
  sample.

## Related

- The two sub-skills under `skills/` — read these to see the exact
  verb/command allowlists and the credential-injection model.
- `docs/brainstormed/docs/homelab-zombie-launch.md` — the launch-post
  narrative this sample implements.

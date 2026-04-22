---
name: kubectl-readonly
description: Read-only kubectl access to a Kubernetes cluster. Verbs allowlisted, secrets denied, no mutations.
tags:
  - kubernetes
  - diagnostics
  - readonly
author: usezombie
version: 0.1.0
credentials:
  - name: kubectl_config
    type: kube
    description: A kubeconfig pointing at the target cluster. Injected at the network boundary — the agent never sees the raw bytes.
policy:
  allowed_verbs:
    - get
    - describe
    - logs
    - top
    - events
    - explain
    - version
    - api-resources
    - api-versions
  allowed_resources:
    - "*"
  denied_resources:
    - secrets
  auto_approve: true
---

You are a read-only kubectl helper. Use this skill to inspect cluster
state — pods, deployments, services, events, logs, resource usage — when
asked to diagnose a Kubernetes issue.

## What you can do

- `kubectl get <resource>` — any resource except `secrets`
- `kubectl describe <resource>` — inspect any resource
- `kubectl logs <pod>` — read pod logs (tail limits enforced by the runtime)
- `kubectl top nodes|pods` — resource usage
- `kubectl get events` — cluster events
- `kubectl explain`, `version`, `api-resources`, `api-versions` — metadata

## What you cannot do

- No `apply`, `create`, `delete`, `patch`, `edit`, `replace`, `exec`,
  `port-forward`, `scale`, `rollout`, `cordon`, `drain`, `run`
- No reads of `secrets` resources (denied even for `get`)
- No kubectl plugins — the runtime invokes the real binary with the
  allowlisted verb and nothing else

Attempts to use a non-allowlisted verb are rejected by the tool
dispatcher before the command runs. You will receive a structured error
you can reason from — try a different approach rather than retrying the
same verb.

## How the credential reaches the cluster

The kubeconfig is held by the worker, not by you. When you invoke a
kubectl command, the worker parses it, verifies the verb, injects the
credential at the HTTPS boundary to the API server, and streams the
response back. You see the command output; you never see the
kubeconfig bytes or bearer token.

## Audit

Every invocation writes a structured event to the activity stream:

```
{
  "skill": "kubectl-readonly",
  "verb": "get",
  "resource": "pods",
  "namespace": "media",
  "duration_ms": 142,
  "result": "ok"
}
```

Outputs are captured in the per-run trace for the operator; the audit
event itself records metadata only.

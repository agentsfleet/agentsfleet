---
name: kubectl-readonly
version: 0.1.0
description: Read-only kubectl access to a Kubernetes cluster. Whitelisted verbs only.
tags: [kubernetes, diagnostics, readonly]
credentials:
  - type: kube
    description: A kubeconfig pointing at the target cluster
policy:
  allowed_verbs: [get, describe, logs, top, events, explain, version, api-resources, api-versions]
  allowed_resources: ["*"]
  denied_resources: [secrets]
  auto_approve: true
---

# kubectl-readonly

A safe, read-only kubectl skill. The agent can investigate a cluster but
cannot write, delete, or expose secret contents.

## What the agent can do

- `kubectl get pods/deployments/services/...` — any resource except secrets
- `kubectl describe ...` — inspect any resource
- `kubectl logs ...` — read pod logs, with tail limits enforced
- `kubectl top ...` — resource usage
- `kubectl get events` — cluster events

## What the agent cannot do

- No `apply`, `create`, `delete`, `patch`, `edit`, `replace`, `exec`,
  `port-forward`, `scale`, `rollout`, `cordon`, `drain`
- No access to `secrets` resources (even read)
- No kubectl plugins — only the binary, only whitelisted verbs

## Implementation

The worker wraps `kubectl` behind a verb/resource allowlist. Every command
the agent issues is parsed, verified against the policy, executed, and
logged. Commands that fail policy return an error to the agent; they are
not silently dropped.

The kubeconfig is injected into the worker's sandbox as a placeholder.
When `kubectl` makes an HTTPS call to the API server, the worker
intercepts at the network boundary, swaps placeholder for real token,
re-originates the call, and streams the response back.

## Audit

Every kubectl invocation writes an audit event:

    {
      "skill": "kubectl-readonly",
      "command": "kubectl get pods -n media",
      "verb": "get",
      "resource": "pods",
      "namespace": "media",
      "duration_ms": 142,
      "result": "ok",
      "output_lines": 8
    }

Outputs are captured but redacted in the audit log (full output available
in the per-run trace).

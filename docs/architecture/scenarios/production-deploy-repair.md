# Scenario — Production deploy repair

> Parent: [`README.md`](./README.md) · References: [`../user_flow.md`](../user_flow.md) §"§8.5 Example: Production deploy repair", [`../capabilities.md`](../capabilities.md), and [`../connectors.md`](../connectors.md).

**Current outcome:** diagnose a failed deployment or prepare a bounded draft Pull Request (PR) for human review. Durable incident linkage and post-deploy verification are unimplemented.

**Target crew:** a responder, repairer and read-only verifier cooperate through events. Grafana and Elasticsearch supply evidence; they are not fleets. No crew row or coordinator fleet exists.

**Proof boundary:** HTTP policy bounds the repairer’s repository writes. Historical M157 specifications describe durable linkage and verification that the current daemon does not implement. Live provider delivery remains separately unproven.

> **Not implemented.** The daemon records no PR link, run history, merge pin, production result or verification: the App ingress drops repair-branch and `deployment_status` traffic as unsupported (`rustd/crates/afd_api_ingress/src/handler/webhook/app_route.rs`), and the storage and dispatcher those would feed were retired (Indy, Oct 15, 2026).
>
> A Slack-requested repair runs under the same grant, the way Claude Tag's attached bundles do; see [`slack-incident-responder.md`](./slack-incident-responder.md) §"7. Four stages, four authorities".

```text
                   Grafana + Elasticsearch
                    ^          ^          ^
                    |          |          |
timer ----------> Fleet 1   Fleet 2    Fleet 3 <----- TARGET: production result
                  responder  repairer    verifier             + 15 minutes
                                        (unimplemented)
                     |          ^
                     v          |
                 diagnosis      +----- failed workflow or human steer
                                |
                                v
                     setup grant -> draft PR -> human merge -> production
```

Fleet 1 never wakes Fleet 2 automatically. Its diagnosis may inform a later human steer. Each Fleet is saved, paused, improved through review, budgeted, and deleted independently.

## Proposed Grafana-only development extension

The [durable repair scenario](durable-repair.md) defines a future development demonstration using Grafana and GitHub without Elasticsearch. It combines scheduled observation, Jev goal decisions, an authorized existing PR, later-CI continuation and worker recovery.
The Elasticsearch-dependent walkthrough below remains a separate scenario, parked outside M224 closure; neither its automatic repair handoff nor post-deployment verification is implemented.

## 1. Start one incident

The responder wakes every fifteen minutes and reads Grafana plus Elasticsearch for new production symptoms. A quiet sweep ends silently. A code-shaped incident produces a diagnosis and a repair intent.

The repairer wakes on a concrete failed GitHub workflow or a human steer. It rereads Grafana and Elasticsearch rather than trusting the responder's prose.

Trigger wiring determines which Fleet runs. No model chooses a crew member, and no stored crew relationship is required.

## 2. Approve before repository write access exists

There is no per-event approval card and no request ceiling. M202 retired both: the `github` grant a person approves while the fleet is set up authorises a write, and the grant, the fleet's `budget.daily_nanos` and `agentsfleet grant delete` bound it (`rustd/crates/afd_gate/src/gate/first.rs:8-17`).

The token is repository-scoped, expires in one hour, carries contents and Pull Request write permissions, and never carries workflow-file permission (`WRITE_REACH` in `rustd/crates/afd_credential/src/credential/github/request.rs:23-26`). The daemon verifies the token returned by GitHub before exposing it to the run.

## 3. Diagnose and open one draft PR

The repairer reads the failed deployment, recent code changes, and current files at a head verified during that run. Provider, secret, or ambiguous failures end diagnosis-only.

For a bounded forward fix, the repairer writes blobs, a tree, a commit, one daemon-issued branch, and one draft PR through GitHub APIs. It never checks out a repository, merges, deploys, or rewinds history.

The branch is `agentsfleet-repair/<repair_ref>`. The repair reference is the unpadded URL-safe Base64 encoding of the leased event identifier's bytes, so its length varies with the identifier (`branch_for` in `rustd/crates/afd_gate/src/policy/repair.rs:65`). The daemon supplies the complete branch in trusted run context; the repairer copies it verbatim and never builds identity metadata itself.

The fleet's reviewed setup declares the exact repository and trusted Pull Request base. The daemon combines them with the event's repair branch and emits generic HTTP rules for host, method, path, and the top-level JSON fields each request may carry, locked or permitted; a rule that names fields refuses any other key and any query string. Those rules allow repository reads, blob and tree creation, a commit carrying only its message, tree and parents, the exact `refs/heads/<repair branch>` ref, and a draft Pull Request whose head and base equal the trusted values. The runner evaluates only those generic rules. It contains no GitHub repair type, procedure, or progress flag.

Instructions reconcile all-state Pull Requests and the exact remote ref before writing.
An existing exact draft may receive bounded corrections on its event-owned branch during the admitted request.
The policy permits only its exact non-force PATCH; this does not admit another event's draft or cross-request continuation.

A validated ref without its Pull Request creates only the missing draft.
When neither exists, the fleet creates Git objects, the exact ref and the draft.

After a timeout or uncertain reply, reread both resources before repeating a write.
GitHub retains remote progress; generic request rules do not guarantee cardinality across several calls.

**Linkage is not ported.** The retired daemon linked a PR opened on the repair branch back to its incident by decoding the gate reference the branch carried. The gate is retired and the Rust App ingress drops repair-branch traffic, so no incident-to-PR link is recorded (the box at the top).

## 4. Keep preview evidence without calling it production

This target is unimplemented. Repair-branch workflows currently create no immutable run history, incident link or verification intent in the daemon.

A future implementation must distinguish preview evidence from production results and correlate the exact merged commit before scheduling verification. Historical M157 specifications describe that target; their guarantees do not describe current behavior.

## 5. Invariants

- Current HTTP rules restrict a write-bound repair lease to its repository, trusted base and daemon-issued branch.
- Reviewed instructions reconcile remote refs and Pull Requests after ambiguous writes; the runner does not enforce a multi-request repair sequence.
- Repair-branch and `deployment_status` traffic are unsupported; neither starts the proposed verifier.
- A human saves the repairer, reviews the diff and merges. The repairer cannot merge, deploy, revert or widen repository access through these HTTP rules.

## 6. Test fixture boundary

`tests/fixtures/fleets/platform-ops/definition.json` remains test input. The API, dashboard, and Command Line Interface (CLI) do not load that directory in production.

The three incident setups sit beside it as fixtures. Save order does not create an ownership edge; event order supplies the workflow.

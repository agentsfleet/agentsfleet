# Scenario — Production deploy repair

> Parent: [`README.md`](./README.md) · References: [`../user_flow.md`](../user_flow.md) §"§8.5 Example: Production deploy repair", [`../capabilities.md`](../capabilities.md), and [`../connectors.md`](../connectors.md).

**Outcome:** a failed production deployment becomes a diagnosis or one bounded draft Pull Request (PR). A human saves the write-bound repairer, reviews the exact diff, and merges. Only a terminal production result for the exact merged commit can wake post-deploy verification.

**Crew shape:** three independently saved Fleets cooperate through events and durable repair linkage. There is no crew row, coordinator Fleet, or vendor-specific Grafana/Elasticsearch Fleet. <img src="https://cdn.simpleicons.org/grafana" width="14" alt="" /> Grafana and <img src="https://cdn.simpleicons.org/elasticsearch" width="14" alt="" /> Elasticsearch are evidence sources read by all three members.

**Proof boundary:** the first two Fleets, approval boundary, draft-PR writer, and initial incident-to-PR linkage ship in M157_002. M157_003 hardens linkage, provenance, append-only run history, merge correlation, and approval spend. It also adds the read-only verifier, durable GitHub production-result intake, standard Fleet result, and deterministic integration proof. The live GitHub App subscription and delivery are not yet proven together; the registration playbook requires that final operator check.

> **Not implemented.** The daemon records no PR link, run history, merge pin, production result or verification: the App ingress drops repair-branch and `deployment_status` traffic as unsupported (`rustd/crates/afd_api_ingress/src/handler/webhook/app_route.rs`), and the storage and dispatcher those would feed were retired (Indy, Oct 15, 2026).
>
> A Slack-requested repair runs under the same grant, the way Claude Tag's attached bundles do; see [`slack-incident-responder.md`](./slack-incident-responder.md) §"7. Four stages, four authorities".

```text
                   Grafana + Elasticsearch
                    ^          ^          ^
                    |          |          |
timer ----------> Fleet 1   Fleet 2    Fleet 3 <----- exact production result
                  responder  repairer    verifier             + 15 minutes
                     |          ^
                     v          |
                 diagnosis      +----- failed workflow or human steer
                                |
                                v
                     setup grant -> draft PR -> human merge -> production
```

Fleet 1 never wakes Fleet 2 automatically. Its diagnosis may inform a later human steer. Each Fleet is saved, paused, improved through review, budgeted, and deleted independently.

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

The fleet's reviewed instructions own remote reconciliation. Before writing, the repairer searches all Pull Request states for the exact repository, head, and base, then reads the exact ref. An existing Pull Request ends with its URL. An existing validated ref with no Pull Request creates only the missing draft Pull Request. When neither exists, it creates Git objects, the exact ref, and the draft Pull Request. A timeout or ambiguous response causes another read before any repeated write. GitHub holds progress across runner restarts; the generic runner rules bind each request but do not claim process-local cardinality.

**Linkage is not ported.** The retired daemon linked a PR opened on the repair branch back to its incident by decoding the gate reference the branch carried. The gate is retired and the Rust App ingress drops repair-branch traffic, so no incident-to-PR link is recorded (the box at the top).

## 4. Keep preview evidence without calling it production

Every completed workflow on the repair branch becomes an immutable run-history row with repository, branch, workflow identity, provider run identifier, head commit hash, conclusion, and completion time. Redelivery is idempotent by provider run identifier.

These rows answer what happened before merge: lint, tests, preview deploys, and any other branch workflow. They never close the incident. A green preview proves only that preview automation accepted the repair branch.

```text
repair branch workflow result
          |
          v
  append immutable history
          |
          +---- visible evidence
          |
          `---- NEVER a closure trigger
```

## 5. Invariants

- One incident can record at most one repair PR per repair Fleet.
- A write-bound repair lease carries provider-neutral HTTP rules for one exact repository, trusted base, and daemon-issued repair branch.
- The repairer's reviewed instructions reconcile the exact remote ref and Pull Request before writes; runner-local state never represents progress.
- The runner contains no GitHub repair module or provider-specific request sequence.
- A repair branch carries the encoded event identifier and no fleet or workspace identity.
- Repair-branch traffic never becomes a fresh incident.
- Preview evidence is append-only and never closes the loop.
- Only exact workspace + repository + merged commit hash correlation can wake verification.
- Production-first, merge-first, simultaneous arrival, and replayed delivery converge on one durable verification attempt.
- A crash between intent and admission leaves a retryable intent, and the retry returns the original Fleet event identifier; it never creates a second verifier event.
- No database connection or row lock remains held during Dragonfly input/output.
- A verifier event is not queued before its fixed fifteen-minute production window is complete.
- Raw `deployment_status` never wakes the verifier; exact correlation schedules `repair_production_result`, and the due dispatcher emits it.
- Production verification requires the platform GitHub App's deployment-status subscription and Deployments read-only permission.
- A signed deployment status proves mapped GitHub origin. Every push-capable identity in the mapped repository is within the trusted producer boundary; this first spine does not attest the producer in daemon code.
- A production result without a commit hash fails closed.
- All three Fleets read Grafana and Elasticsearch; those vendors do not become Fleets.
- Every matching verifier Fleet receives its own attempt; no name, role, or crew resolver chooses one.
- The verifier has no repository write permission.
- A human saves the write-bound repairer, reviews the diff, and merges.
- No repair automatically merges, deploys, reverts, or expands to a second repository.

## 6. Test fixture boundary

`tests/fixtures/fleets/platform-ops/definition.json` remains test input. The API, dashboard, and Command Line Interface (CLI) do not load that directory in production.

The three incident setups sit beside it as fixtures. Save order does not create an ownership edge; event order supplies the workflow.

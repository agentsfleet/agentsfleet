# Scenario — Production deploy repair

> Parent: [`README.md`](./README.md) · References: [`../user_flow.md`](../user_flow.md) §8.5, [`../capabilities.md`](../capabilities.md), and [`../connectors.md`](../connectors.md).

**Outcome:** a failed production deployment becomes a diagnosis or one bounded draft Pull Request (PR). A human approves repository write access, reviews the exact diff, and merges. Only a terminal production result for the exact merged commit can wake post-deploy verification.

**Crew shape:** three independently installed Fleets cooperate through events and durable repair linkage. There is no crew row, coordinator Fleet, or vendor-specific Grafana/Elasticsearch Fleet. <img src="https://cdn.simpleicons.org/grafana" width="14" alt="" /> Grafana and <img src="https://cdn.simpleicons.org/elasticsearch" width="14" alt="" /> Elasticsearch are evidence sources read by all three members.

**Proof boundary:** the first two Fleets, approval boundary, draft-PR writer, and initial incident-to-PR linkage ship in M157_002. M157_003 hardens linkage, provenance, append-only run history, merge correlation, and approval spend. It also adds the read-only verifier, durable GitHub production-result intake, standard Fleet result, and deterministic integration proof. The live GitHub App subscription and delivery are not yet proven together; the registration playbook requires that final operator check.

Legend: ✅ implemented and tested · 🟡 being hardened · 🔨 specified, not built.

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
                     approve -> draft PR -> human merge -> production
```

Fleet 1 never wakes Fleet 2 automatically. Its diagnosis may inform a later human steer. Each Fleet is installed, paused, updated, budgeted, and deleted independently.

## 1. Start one incident

The responder wakes every fifteen minutes and reads Grafana plus Elasticsearch for new production symptoms. A quiet sweep ends silently. A code-shaped incident produces a diagnosis and a repair intent.

The repairer wakes on a concrete failed GitHub workflow or a human steer. It rereads Grafana and Elasticsearch rather than trusting the responder's prose. Its wake parks behind repository-write approval before any run starts.

Trigger wiring determines which Fleet runs. No model chooses a crew member, and no stored crew relationship is required.

## 2. Approve before repository write access exists

A Fleet whose repository binding declares write access parks every first-encounter event before gate rules are evaluated. The approval card states the repository, trusted Pull Request base, permissions, and a ceiling of 32 write-credential requests. Approval releases the run, but every request reserves one use atomically before the daemon reads a secret or calls GitHub. A failed request still consumes its reserved use; exhaustion requires a new human approval.

The token is repository-scoped, expires in one hour, carries contents and Pull Request write permissions, and never carries workflow-file permission. The daemon verifies the token returned by GitHub before exposing it to the run.

## 3. Diagnose and open one draft PR

The repairer reads the failed deployment, recent code changes, and current files at a head verified during that run. Provider, secret, or ambiguous failures end diagnosis-only.

For a bounded forward fix, the repairer writes blobs, a tree, a commit, one daemon-issued branch, and one draft PR through GitHub APIs. It never checks out a repository, merges, deploys, or rewinds history.

The branch is `agentsfleet-repair/<repair_ref>`. The repair reference is the unpadded URL-safe Base64 encoding of the approved repository-write gate's 16 raw Universally Unique Identifier version 7 (UUIDv7) bytes. It is exactly 22 characters, so the complete branch is 41 characters. The daemon supplies the complete branch in trusted run context; the repairer copies it verbatim and never builds identity metadata itself.

The user-authored `TRIGGER.md` declares the exact repository and trusted Pull Request base. The daemon combines them with the approved gate's repair branch and emits generic HTTP rules for host, method, path, and locked top-level JSON fields. Those rules allow repository reads, Git object creation, the exact `refs/heads/<repair branch>` ref, and a draft Pull Request whose head and base equal the trusted values. The runner evaluates only those generic rules. It contains no GitHub repair type, procedure, or progress flag.

The user-authored `SKILL.md` owns remote reconciliation. Before writing, the repairer searches all Pull Request states for the exact repository, head, and base, then reads the exact ref. An existing Pull Request ends with its URL. An existing validated ref with no Pull Request creates only the missing draft Pull Request. When neither exists, it creates Git objects, the exact ref, and the draft Pull Request. A timeout or ambiguous response causes another read before any repeated write. GitHub holds progress across runner restarts; the generic runner rules bind each request but do not claim process-local cardinality.

When the PR-opened webhook returns, the daemon:

1. decodes the repair reference and loads that exact approved write gate;
2. reads the gate's workspace, repair Fleet, incident event, and repository binding;
3. verifies the composite Fleet-plus-event row, repository, installation, base repository, and author provenance;
4. records the incident-to-PR link; and
5. drops the repair branch from normal incident routing.

The repair reference is correlation, not authorization. A malformed reference, an unknown gate, a gate that did not approve repository write, or a mismatched event or repository fails closed and records no repair link.

The shared GitHub App ingress and the signed per-Fleet webhook call the same linkage arm.

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

## 5. Pin the merge before observing production

The human reviews the PR diff and merges through GitHub. A merged PR webhook records GitHub's exact `merge_commit_sha` on the repair link. Merge, squash, and rebase strategies are all represented by that provider-returned value; the daemon never guesses it from the current default branch.

If the webhook says closed-but-not-merged, or omits the merged commit hash, the link remains unmerged and cannot correlate a production result. The production result may arrive first; it remains durable until a matching merge exists.

```text
PR opened -> incident link

merged_commit_sha -------------------+
                                      +-- exact equality --> verifier event allowed
production repository + commit ------+

Either merge or production may arrive first; both are durable.
```

## 6. Wake the verifier only for the exact production result

The verifier is a third, read-only Fleet. GitHub deployment-status intake runs before Fleet trigger matching. A Vercel deployment is eligible only when Vercel reports it through GitHub's deployment-status event.

Every normalized production result is stored before correlation. The same reconciler runs after either a production-result insert or a merged-hash write, so webhook order does not change the outcome. Both paths take the same transaction-scoped PostgreSQL advisory lock keyed by workspace, repository, and commit before writing or reconciling. A simultaneous arrival therefore waits, then observes the first committed side instead of both deliveries missing each other. Before emitting an event, the daemon requires all of the following:

- the provider marks the deployment terminal;
- the environment is production;
- repository identity is exact;
- the provider supplies the deployed commit hash; and
- that hash equals a stored `merge_commit_sha` in the same workspace.

A successful match records one verification attempt. When its fixed window completes, the dispatcher emits one internal `repair_production_result` event. Normal Fleet routing then selects every installed Fleet subscribed to that proof-qualified event type. The verifier subscribes to `repair_production_result`, never raw `deployment_status`; no Fleet name, role, or crew lookup is introduced.

Each selected verifier Fleet gets one slot 835 dispatch intent before Redis is called. The row starts with `verifier_event_id = NULL` and sets `verify_after` to fifteen minutes after production completion. Its row identifier is the stable dispatch key. A bounded background dispatcher selects due rows. One failed row is logged and retried on the next sweep without blocking later due rows. Redis atomically appends the Fleet event and remembers the generated stream event identifier, or returns the identifier from an earlier attempt with the same key. The daemon then fills `verifier_event_id` once. A later cleanup sweep deletes the transient Redis once-key and records that cleanup in slot 835. Cleanup retries are safe because the durable event link already prevents another dispatch. The dispatcher releases its database connection before every Redis call.

```text
slot 835 intent          Redis enqueue-once          slot 835 complete       cleanup
event id = NULL    ---> new or existing event id ---> event id = <id>  ---> delete once-key
verify_after = +15m          only when due                                      |
          ^                         |                                            |
          |                         |                                            v
          `------ bounded retry ----+                              record cleanup in slot 835

crash before Redis  -> pending intent is retried
crash after Redis   -> retry returns the same event id
crash during cleanup -> deletion and cleanup record are retried safely
```

The `verifier_event_id` is therefore the standard Fleet event identifier for Fleet 3's verification run. It is not another incident identifier and users do not copy it between Fleets. It lets event history, logs, and support trace the exact verification run back to the repair and production result.

A missing or mismatched hash remains durable with a named unmatched reason and emits no synthetic event. A preview result remains history even if it is green. A default-branch result for a later commit does not verify an earlier repair.

The queued verifier event is enriched with the incident event identifier, repair PR, merged commit hash, provider deployment identifier, production completion time, and evidence window. The Fleet receives this context; it does not read internal database tables.

## 7. Judge production telemetry read-only

The verifier event reuses the linked incident request, repair result, and Pull Request evidence already stored by the normal Fleet path. Fifteen minutes after production completion, the verifier reads Grafana and Elasticsearch over that completed window and judges the original symptom. There is no configurable baseline engine or separate settling period.

- `cleared` — the original symptom is absent through the complete fifteen-minute window;
- `not_cleared` — the original symptom remains or regressed; or
- `inconclusive` — evidence is missing, contradictory, or the window is incomplete.

Its repository binding is read-only and pinned to the exact merged commit carried by the event. It must never inspect whatever commit happens to be current when the Fleet runs.

The standard Fleet event stores the verifier's response and repair context. Operators read `cleared`, `not_cleared`, or `inconclusive` from that response; the daemon does not parse model prose into another status. This workstream adds no separate incident card. Human review and merge remain mandatory; verification never auto-merges or auto-reverts.

## 8. Production-result normalization

Production results enter as signed GitHub deployment-status events. This includes Vercel deployments surfaced through GitHub. Direct Vercel webhook ingestion is outside this repair loop. GitHub input normalizes before correlation:

```text
production_result {
  provider, provider_deployment_id, provider_status_id,
  workspace_id, repository,
  environment, commit_sha,
  conclusion, completed_at
}
```

The platform GitHub App subscribes to deployment-status events and holds Deployments read-only permission. Development registration proves one signed delivery reaches `/v1/ingress/github` before the same setting is applied to production. The live record includes `deployment.id` as deployment context and `deployment_status.id` as slot 834's append identity. Fixture coverage is not accepted as evidence that the live App subscription exists.

`agentsfleet` accepts any signed terminal production status from a mapped GitHub installation. That proves GitHub origin and repository routing; it does not attest that Vercel produced the status. GitHub permits every push-capable identity to create deployment statuses, so each such identity in a mapped repository is inside this first spine's trusted producer boundary. The daemon does not inspect `deployment_status.creator` or App identity. The live proof records the expected deployment integration, received creator identity, repository, commit, deployment identifier, deployment-status identifier, and delivery identifier in Pull Request (PR) Session Notes for audit; it does not add a daemon rejection rule.

Slot 834 retains every normalized production result idempotently by provider status identifier (`deployment_status.id`). It also retains the provider deployment identifier (`deployment.id`) as correlation evidence. Slot 835 retains each correlated verification attempt, its fixed `verify_after`, nullable-then-final `verifier_event_id`, claim fence, and Redis cleanup marker. The same reconciler reads both repair merges and production results under their shared transaction lock, so result-first, merge-first, simultaneous delivery, replayed delivery, and process restart converge on one attempt and one Fleet event per matching verifier Fleet. Two repair links for the same exact commit are ambiguous: correlation logs the ambiguity and creates no closure event. Several matching verifier installations intentionally produce several independent results; normal trigger configuration narrows that set without a crew resolver. An exact correlation schedules `repair_production_result` with the matched incident request and response, repair evidence, merged commit, production result, and fixed evidence window. Provider vocabulary is translated only at ingress. Verifier routing and prompting remain independent of the deployment vendor. A payload without exact repository, environment, or commit identity fails closed and emits nothing.

## 9. What exists and what changes

| Part | Status | Evidence or owning workstream |
|---|---|---|
| Incident responder Fleet | ✅ | `library/incident-responder/`; scheduled Grafana and Elasticsearch diagnosis. |
| Incident repairer Fleet | ✅ | `library/incident-repairer/`; approval-gated draft PR. |
| Write-kind approval park and fenced mint | ✅ | M157_002 integration coverage. |
| Incident-to-PR linkage | 🟡 | Slot 830 exists; M157_003 moves it onto shared ingress and adds provenance. |
| Append-only workflow history | 🟡 | M157_003, slot 831. |
| Exact merged-commit correlation | 🟡 | M157_003, slot 832. |
| Bounded approval mint spends | 🟡 | M157_003, slot 833. |
| Incident verifier Fleet | 🟡 | M157_003; independently installed and read-only. |
| GitHub production-result normalization | 🟡 | M157_003; includes Vercel deployments surfaced through GitHub. |
| GitHub App deployment subscription and permission | 🔨 | M157_003 operator playbook plus development live-delivery proof. |
| Durable production-result ledger and order-independent reconciler | 🟡 | M157_003, slots 834–835. |
| Proof-qualified `repair_production_result` event | 🟡 | M157_003; emitted only after exact repair correlation. |

## 10. Invariants

- One incident can record at most one repair PR per repair Fleet.
- A write-bound repair lease carries provider-neutral HTTP rules for one exact repository, trusted base, and daemon-issued repair branch.
- The repairer's user-authored skill reconciles the exact remote ref and Pull Request before writes; runner-local state never represents progress.
- The runner contains no GitHub repair module or provider-specific request sequence.
- A repair branch carries one 22-character daemon-issued gate reference, never raw Fleet-plus-event identifiers.
- A repair reference resolves one approved write gate and one exact Fleet-plus-event row or records nothing.
- Repair-branch traffic never becomes a fresh incident.
- Preview evidence is append-only and never closes the loop.
- Only exact workspace + repository + merged commit hash correlation can wake verification.
- Production-first, merge-first, simultaneous arrival, and replayed delivery converge on one durable verification attempt.
- A PostgreSQL-to-Redis crash leaves a retryable intent or returns the original Fleet event identifier; it never creates a second verifier event.
- No database connection or row lock remains held during Redis input/output.
- The transient Redis once-key is deleted only after durable event completion; interrupted cleanup is retried and cannot create another event.
- A verifier event is not queued before its fixed fifteen-minute production window is complete.
- Raw `deployment_status` never wakes the verifier; exact correlation schedules `repair_production_result`, and the due dispatcher emits it.
- Production verification requires the platform GitHub App's deployment-status subscription and Deployments read-only permission.
- A signed deployment status proves mapped GitHub origin. Every push-capable identity in the mapped repository is within the trusted producer boundary; this first spine does not attest the producer in daemon code.
- A production result without a commit hash fails closed.
- All three Fleets read Grafana and Elasticsearch; those vendors do not become Fleets.
- Every matching verifier Fleet receives its own attempt; no name, role, or crew resolver chooses one.
- The verifier has no repository write permission.
- A human approves write access, reviews the diff, and merges.
- No repair automatically merges, deploys, reverts, or expands to a second repository.

## 11. Test fixture boundary

`tests/fixtures/fleetbundle/platform-ops` remains test input. The API, dashboard, and Command Line Interface (CLI) do not load that directory in production.

The shipped members live in `library/` and install through the normal library endpoints: `incident-responder`, `incident-repairer`, then `incident-verifier`. Installation order does not create an ownership edge; event order supplies the workflow.

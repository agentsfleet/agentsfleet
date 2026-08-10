# Scenario — Production deploy repair

> Parent: [`README.md`](./README.md) · References: [`../user_flow.md`](../user_flow.md) §8.5, [`../capabilities.md`](../capabilities.md), and [`../connectors.md`](../connectors.md).

**Outcome:** a failed production deployment becomes a diagnosis or one bounded draft Pull Request (PR). A human approves repository write access, reviews the exact diff, and merges. Only a terminal production result for the exact merged commit can wake post-deploy verification.

**Crew shape:** three independently installed Fleets cooperate through events and durable repair linkage. There is no crew row, coordinator Fleet, or vendor-specific Grafana/Elasticsearch Fleet. Grafana and Elasticsearch are evidence sources read by all three members.

**Proof boundary:** the first two Fleets, approval boundary, draft-PR writer, and initial incident-to-PR linkage ship in M157_002. M157_003 hardens linkage, provenance, append-only run history, merge correlation, and approval spend. M157_004 adds the read-only verifier, GitHub production-result intake, operator result surface, and live-repository proof.

Legend: ✅ implemented and tested · 🟡 being hardened · 🔨 specified, not built.

```text
          Grafana + Elasticsearch (read-only evidence)
                    ^              ^              ^
                    |              |              |
       +------------+--+    +------+---------+    +--------+---------+
       | Fleet 1       |    | Fleet 2        |    | Fleet 3          |
       | responder     |--->| repairer       |--->| verifier         |
       | every 15 min  |    | failure/manual |    | production only  |
       | diagnose      |    | draft PR       |    | cleared?         |
       +---------------+    +------+---------+    +---------+--------+
                                   |                        ^
                                   v                        |
                            human approve/review            |
                                   |                        |
                                   v                        |
                             merge exact bytes              |
                                   |                        |
                                   v                        |
                   repository + merged commit hash ---------+
```

The arrows show event order, not ownership. Each Fleet is installed, paused, updated, budgeted, and deleted independently.

## 1. Start one incident

The responder wakes every fifteen minutes and reads Grafana plus Elasticsearch for new production symptoms. A quiet sweep ends silently. A code-shaped incident produces a diagnosis and a repair intent.

The repairer wakes on a concrete failed GitHub workflow or a human steer. It rereads Grafana and Elasticsearch rather than trusting the responder's prose. Its wake parks behind repository-write approval before any run starts.

Trigger wiring determines which Fleet runs. No model chooses a crew member, and no stored crew relationship is required.

## 2. Approve before repository write access exists

A Fleet whose repository binding declares write access parks every first-encounter event before gate rules are evaluated. The approval card states the repository, permissions, and mint ceiling. Approval releases the run, but each token mint atomically spends one unit from that ceiling.

The token is repository-scoped, expires in one hour, carries contents and Pull Request write permissions, and never carries workflow-file permission. The daemon verifies the token returned by GitHub before exposing it to the run.

## 3. Diagnose and open one draft PR

The repairer reads the failed deployment, recent code changes, and current files at a head verified during that run. Provider, secret, or ambiguous failures end diagnosis-only.

For a bounded forward fix, the repairer writes blobs, a tree, a commit, one daemon-issued branch, and one draft PR through GitHub APIs. It never checks out a repository, merges, deploys, or rewinds history.

The branch is `agentsfleet-repair/<repair_ref>`. The repair reference is the unpadded URL-safe Base64 encoding of the approved repository-write gate's 16 raw Universally Unique Identifier version 7 (UUIDv7) bytes. It is exactly 22 characters, so the complete branch is 41 characters. The daemon supplies the complete branch in trusted run context; the repairer copies it verbatim and never builds identity metadata itself.

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

If the webhook says closed-but-not-merged, or omits the merged commit hash, the link remains unmerged and cannot correlate a production result.

```text
PR opened       PR merged                     production result
   |                 |                              |
   v                 v                              v
incident link   merged_commit_sha      repository + commit_sha + environment
                         \                         /
                          +---- exact equality ----+
                                    |
                                    v
                           verifier event allowed
```

## 6. Wake the verifier only for the exact production result

The verifier is a third, read-only Fleet. GitHub deployment-status intake runs before Fleet trigger matching. A Vercel deployment is eligible only when Vercel reports it through GitHub's deployment-status event.

Before queueing an event, the daemon requires all of the following:

- the provider marks the deployment terminal;
- the environment is production;
- repository identity is exact;
- the provider supplies the deployed commit hash; and
- that hash equals a stored `merge_commit_sha` in the same workspace.

A successful match emits one internal `repair_production_result` event. Normal Fleet routing then selects every installed Fleet subscribed to that proof-qualified event type. The verifier subscribes to `repair_production_result`, never raw `deployment_status`; no Fleet name, role, or crew lookup is introduced.

A missing or mismatched hash records an ignore reason and emits no synthetic event. A preview result remains history even if it is green. A default-branch result for a later commit does not verify an earlier repair.

The queued verifier event is enriched with the incident event identifier, repair PR, merged commit hash, provider deployment identifier, production completion time, and evidence window. The Fleet receives this context; it does not read internal database tables.

## 7. Judge production telemetry read-only

The verifier reads Grafana and Elasticsearch after the production completion time, compares the same incident signals with the pre-deploy baseline, and reports one of:

- `cleared` — the original symptom is absent through the configured observation window;
- `not_cleared` — the original symptom remains or regressed; or
- `inconclusive` — evidence is missing, contradictory, or the window is incomplete.

Its repository binding is read-only and pinned to the exact merged commit carried by the event. It must never inspect whatever commit happens to be current when the Fleet runs.

The standard Fleet event stores the verifier's response. A repair-verification link joins that event back to the incident and PR for the operator surface. Human review and merge remain mandatory; verification never auto-merges or auto-reverts.

## 8. Production-result normalization

Production results enter as signed GitHub deployment-status events. This includes Vercel deployments surfaced through GitHub. Direct Vercel webhook ingestion is outside this repair loop. GitHub input normalizes before correlation:

```text
production_result {
  provider, provider_deployment_id,
  workspace_id, repository,
  environment, commit_sha,
  conclusion, completed_at
}
```

The platform GitHub App subscribes to deployment-status events and holds Deployments read-only permission. Development registration proves one signed delivery reaches `/v1/ingress/github` before the same setting is applied to production. Fixture coverage is not accepted as evidence that the live App subscription exists.

An exact correlation emits `repair_production_result` with the matched incident, repair PR, merged commit, production result, and evidence window. Provider vocabulary is translated only at ingress. Verifier routing and prompting remain independent of the deployment vendor. A payload without exact repository, environment, or commit identity fails closed and emits nothing.

## 9. What exists and what changes

| Part | Status | Evidence or owning workstream |
|---|---|---|
| Incident responder Fleet | ✅ | `library/incident-responder/`; scheduled Grafana and Elasticsearch diagnosis. |
| Incident repairer Fleet | ✅ | `library/incident-repairer/`; approval-gated draft PR. |
| Write-kind approval park and fenced mint | ✅ | M157_002 integration coverage. |
| Incident-to-PR linkage | 🟡 | Slot 830 exists; M157_003 moves it onto shared ingress and adds provenance. |
| Append-only workflow history | 🔨 | M157_003, slot 831. |
| Exact merged-commit correlation | 🔨 | M157_003, slot 832. |
| Bounded approval mint spends | 🔨 | M157_003, slot 833. |
| Incident verifier Fleet | 🔨 | M157_004; independently installed and read-only. |
| GitHub production-result normalization | 🔨 | M157_004; includes Vercel deployments surfaced through GitHub. |
| GitHub App deployment subscription and permission | 🔨 | M157_004; operator playbook plus development live-delivery proof. |
| Proof-qualified `repair_production_result` event | 🔨 | M157_004; emitted only after exact repair correlation. |
| Incident → verifier-result operator surface | 🔨 | M157_004. |
| Live-repository acceptance arc | 🔨 | M157_004. |

## 10. Invariants

- One incident can record at most one repair PR per repair Fleet.
- A repair branch carries one 22-character daemon-issued gate reference, never raw Fleet-plus-event identifiers.
- A repair reference resolves one approved write gate and one exact Fleet-plus-event row or records nothing.
- Repair-branch traffic never becomes a fresh incident.
- Preview evidence is append-only and never closes the loop.
- Only exact workspace + repository + merged commit hash correlation can wake verification.
- Raw `deployment_status` never wakes the verifier; exact correlation emits `repair_production_result` first.
- Production verification requires the platform GitHub App's deployment-status subscription and Deployments read-only permission.
- A production result without a commit hash fails closed.
- All three Fleets read Grafana and Elasticsearch; those vendors do not become Fleets.
- The verifier has no repository write permission.
- A human approves write access, reviews the diff, and merges.
- No repair automatically merges, deploys, reverts, or expands to a second repository.

## 11. Test fixture boundary

`tests/fixtures/fleetbundle/platform-ops` remains test input. The API, dashboard, and Command Line Interface (CLI) do not load that directory in production.

The shipped members live in `library/` and install through the normal library endpoints: `incident-responder`, `incident-repairer`, then `incident-verifier`. Installation order does not create an ownership edge; event order supplies the workflow.

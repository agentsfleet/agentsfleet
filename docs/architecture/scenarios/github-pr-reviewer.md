# Scenario — GitHub PR reviewer (the golden path)

> Parent: [`README.md`](./README.md) · User-facing: [docs.agentsfleet.net/quickstart](https://docs.agentsfleet.net/quickstart) and [docs.agentsfleet.net/fleets/connectors](https://docs.agentsfleet.net/fleets/connectors).

> References: [`../api_resources.md`](../api_resources.md) (drafts, Save and the Library), [`../data_flow.md`](../data_flow.md) (trigger/execute loop), [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) (provider posture + credit gate).
>
> It follows one persona — **John Doe** — saving the `github-pr-reviewer` fleet through terminal chat in the Command-Line Interface (CLI), connecting the shared GitHub App to his workspace, binding a repository to the fleet, and watching the fleet read a Pull Request (PR); the review post is refused at egress today (§4). Provider posture, billing math, and the credit gate are not re-narrated here; those facts live in their topic docs.

**Outcome under test:** from a published GitHub Pull Request reviewer Library revision to a posted review comment, with the fleet running its reviewed instructions against a repository-bound App event and using a short-lived installation token for runtime GitHub API calls. Today the run reads the diff and the review post is refused at egress (§4). This scenario is **not yet proven**: it becomes green only when the repository-bound Pull Request integration test passes end to end.

Legend: ✅ implemented and locally proven · 🔨 not built or not proven.

```mermaid
sequenceDiagram
  autonumber
  participant Admin as Platform admin
  participant Op as Workspace user
  participant CLI as agentsfleet
  participant API as agentsfleetd
  participant GH as GitHub
  participant PG as Postgres
  participant Runner as agentsfleet-runner

  Admin->>GH: create App; set callback + /v1/ingress/github
  Admin->>API: vault App identity, webhook secret, and client credentials
  Note over Admin,PG: admin already staged and published github-pr-reviewer from a reviewed chat draft → authoring.library_revisions
  Op->>API: sign up and create/select workspace W
  Op->>API: connect github (signed single-use state)
  API-->>Op: GitHub user-authorization URL
  Note over Op,GH: App must ALREADY be installed — no install-page continuation
  Op->>GH: authorize as the signed-in person
  GH->>API: callback one-time code + state (+ installation_id when claimed)
  API->>GH: exchange code; verify user can access installation
  GH-->>API: installation accessible
  API->>PG: conditional vault handle + connector.installs route
  Note over API,PG: other workspace already owns installation → 403, no mutation
  Op->>CLI: chat --library github-pr-reviewer --revision <REVISION_ID> --model <MODEL_REF>
  CLI->>API: POST /v1/workspaces/{ws}/drafts { source: library revision }
  Op->>CLI: ask for repositories=[acme/payments], events=[pull_request]; review
  CLI->>API: GET draft review (setup, access, readiness, review digest)
  CLI->>API: POST /v1/workspaces/{ws}/fleets { source: draft candidate, review_digest }
  API->>PG: INSERT fleet.fleets (installing) + setup revision 1
  API-->>CLI: { fleet, setup checks: github grant pending }
  Op->>API: approve the github grant on the setup check
  Op->>CLI: resume <FLEET_ID>
  CLI->>API: PATCH fleet active with current If-Match
  API->>PG: locked setup recheck ⇒ active
  Note over Op,Runner: …a PR is opened…
  GH->>API: POST /v1/ingress/github
  API->>API: verify App signature before payload routing
  API->>PG: installation → workspace → repository/event/grant fleets
  API->>PG: INSERT fleet.admissions (webhook_app, fleet:body digest)
  API->>API: best-effort sharded wake
  Runner->>API: lease → { instructions, event, policy } (no sandbox: every tool runs in the supervisor)
  Runner->>GH: GET /pulls/{n} (Accept: vnd.github.diff, Bearer ${secrets.github.token})
  Note over Runner,GH: POST /pulls/{n}/reviews refused at egress, never sent ❌
  Runner->>API: report → event processed → dashboard event stream
```

---

## 1. Save — from a Library revision to a fleet

A platform admin publishes a reviewed draft candidate through `POST` and `PATCH /v1/admin/fleet-libraries`. John opens a private draft pinned to that revision:

```bash
agentsfleet chat --library github-pr-reviewer --revision <REVISION_ID> --model <MODEL_REF>
```

He requests his repository, reviews the exact setup, then saves through `POST /v1/workspaces/{ws}/fleets`.

Get `<MODEL_REF>` from the [authoring model list](../user_flow.md#80-the-wedge-surface), and `<REVISION_ID>` from the Library entry. Save returns `<FLEET_ID>`; approval alone does not resume setup.

## 2. Two layers: the immutable revision and the live fleet

The fleet runs the instructions and policy of its selected setup revision in `fleet.revisions`, read fresh by every claim (`rustd/crates/afd_fleet/src/lease/installed.rs`). The Library revision it started from stays immutable; a later improvement appends a new fleet revision through review and changes no Library entry.

## 3. Connect the App, bind the repository, then receive the PR

1. **Platform setup and workspace connection.** The shared App's setup and John's user-authorization flow are in [`../connectors.md`](../connectors.md) §"GitHub App: platform setup to fleet execution". The App must already be installed on `acme/payments`; an installation already owned by another workspace returns 403 without changing either workspace.
2. **Fleet subscription.** The saved fleet's webhook trigger declares `source: github`, `events: [pull_request]`, and `repositories: [acme/payments]` in its reviewed setup. The App installation is the widest set of repositories; this list picks the ones this fleet hears. A fleet with no `repositories` list receives no App traffic (`serves_repository` in `rustd/crates/afd_ingress/src/binding/mod.rs:250-256`).
3. **A PR is opened.** GitHub signs and posts the event to `/v1/ingress/github`. The receiver verifies before reading routing fields, then resolves the installation.

   It selects only active and approved fleets matching `acme/payments` plus `pull_request`. Each fleet's `fleet.admissions` row is keyed by fleet and body digest. A best-effort notification prioritizes pickup; every poll also queries Postgres ([`../lease_flow.md`](../lease_flow.md#admission-commits-the-queue-row)).

4. **The authorisation, once, at Save.** Save raises a `github` grant check. John approves it, then runs `agentsfleet resume <FLEET_ID>`; activation rechecks all setup checks under lock.

   Approval alone does not resume setup (`rustd/crates/afd_authoring/src/creation/patch.rs`). Grant scope and revocation belong in [connector trust anchors](../connectors.md#trust-anchors). To narrow repository reach, apply a reviewed improvement with a shorter `repositories` list.

   There is no per-event approval. A repository-write gate was raised on every first-encounter event until M202: a continuation carries a fresh event identifier, so each model turn re-parked, and one steer produced three cards and zero review comments. The grant replaced it because the grant already carried everything the card asked about.

The manual `/v1/webhooks/{fleet_id}/github` route remains available for an operator-managed per-fleet hook. It verifies with the `webhook_secret` field of the workspace's `github` secret, does not require `repositories`, and is not the default App path (`rustd/crates/afd_api_ingress/src/handler/webhook/github_route.rs`).

## 4. The run — the reviewed instructions drive the review

A runner leases the event, and the model reads the PR diff through `http_request` with a per-lease installation token substituted at egress. The review POST is refused at egress with `RequestPolicyNotAllowed`, because no write rule admits `/pulls/{n}/reviews` and instructions cannot widen the rules ([`../lease_flow.md`](../lease_flow.md) §"3. `github-pr-reviewer` end to end" and §"4. How AGENT BOB 01 can reply").

The gate + billing path is identical to every other event — see [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) for the credit-pool deductions and the gate.

## 5. What John sees after the integration test proves the path

- Not yet: the pull request carries no review, because the review POST is refused at egress (§4).
- `agentsfleet events {id}` / the dashboard `/fleets/{id}` thread shows the run: the `http_request` tool calls and the response, streamed over Server-Sent Events (SSE), durable in `fleet.events`.

## 6. Proof status

Everything but the external proof is green: the native setup install, App
callback and reconnect, ingress filtering by installation / repository / event /
grant, instructions delivery per lease, the diff read, and the local
repository-bound `pull_request` suite against real Postgres and Dragonfly.

Three remain open, and the scenario is not fixed until the first two pass:

- **The review post.** No egress write rule admits `/pulls/{n}/reviews` or an
  issue comment, so the run reads the diff and cannot answer (§4). Parked for
  Indy to decide after running it: `docs/v2/done/M210_002_P1_API_INFRA_RUST_RUNNER_AGENT_LOOP_AND_HOSTED_TOOLS.md`, Dimension 6.3.

- **External `github-pr-reviewer` repository test.** Needs the App installed on
  a dedicated development repository and a real Pull Request. Fixture coverage
  is not evidence that the live path works.
- **Compounding memory across Pull Requests.** Parked design.

Milestone status belongs to the spec, not to this page. When the external proof
lands, the spec records it and this section says so in one line.

## 7. What is NOT in this scenario

- **Provider posture, billing math, the credit gate.** These had their own scenarios; the canonical facts now live in [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md). The lease/execute/bill loop is unchanged from what that doc describes.
- **Compounding memory** across PRs. A separate, parked design.

## Proposed repair and review separation

The [durable repair scenario](durable-repair.md) allows a future repair fleet to update an explicitly authorized externally opened PR. That does not grant review-posting authority or allow a second fleet to push independently.
A future reviewer must bind findings to an exact head and use its own reviewed publication authority; current review-posting limitations above remain in force.

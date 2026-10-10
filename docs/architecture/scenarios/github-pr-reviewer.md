# Scenario — GitHub PR reviewer (the golden path)

> Parent: [`README.md`](./README.md) · User-facing: [docs.agentsfleet.net/quickstart](https://docs.agentsfleet.net/quickstart) and [docs.agentsfleet.net/fleets/connectors](https://docs.agentsfleet.net/fleets/connectors).

> References: [`../fleet_bundles.md`](../fleet_bundles.md) (bundle storage), [`../data_flow.md`](../data_flow.md) (trigger/execute loop), [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) (provider posture + credit gate).
>
> This is the single end-to-end walkthrough. It follows one persona — **John Doe** — installing the `github-pr-reviewer` fleet through the Command-Line Interface (CLI), connecting the shared GitHub App to his workspace, binding a repository to the fleet, and watching the fleet read a Pull Request (PR); the review post is refused at egress today (§4). Provider posture, billing math, and the credit gate are not re-narrated here; those facts live in their topic docs.

**Outcome under test:** from a GitHub Pull Request reviewer template to a posted review comment, with the fleet running its installed `SKILL.md` against a repository-bound App event and using a short-lived installation token for runtime GitHub API calls. Today the run reads the diff and the review post is refused at egress (§4). This scenario is **not yet proven**: it becomes green only when the repository-bound Pull Request integration test passes end to end.

Legend: ✅ implemented and locally proven · 🔨 not built or not proven.

```mermaid
sequenceDiagram
  autonumber
  participant Admin as Platform admin
  participant Op as Workspace user
  participant CLI as agentsfleet
  participant API as agentsfleetd-api
  participant GH as GitHub
  participant R2 as R2 (object store)
  participant PG as Postgres
  participant Runner as agentsfleet-runner

  Admin->>GH: create App; set callback + /v1/ingress/github
  Admin->>API: vault App identity, webhook secret, and client credentials
  Note over Admin,PG: admin already onboarded github-pr-reviewer → R2 + core.fleet_library
  Op->>API: sign up and create/select workspace W
  Op->>API: connect github (signed single-use state)
  API-->>Op: GitHub user-authorization URL
  Note over Op,GH: App must ALREADY be installed — no install-page continuation
  Op->>GH: authorize as the signed-in person
  GH->>API: callback one-time code + state (+ installation_id when claimed)
  API->>GH: exchange code; verify user can access installation
  GH-->>API: installation accessible
  API->>PG: conditional vault handle + connector_installs route
  Note over API,PG: other workspace already owns installation → 403, no mutation
  Op->>CLI: install --library github-pr-reviewer
  CLI->>API: GET /v1/workspaces/{ws}/fleet-libraries
  API-->>CLI: platform row { id:"github-pr-reviewer", visibility:"platform" }
  CLI->>API: POST /v1/workspaces/{ws}/fleets { platform_library_id:"github-pr-reviewer" }
  API->>PG: INSERT core.fleets
  API-->>CLI: { fleet_id }
  Op->>API: TRIGGER: repositories=[acme/payments], events=[pull_request]
  Note over Op,Runner: …a PR is opened…
  GH->>API: POST /v1/ingress/github
  API->>API: verify App signature before payload routing
  API->>PG: installation → workspace → repository/event/grant fleets
  API->>API: claim body-digest+fleet replay slot → XADD fleet:{id}:events ✅
  Runner->>API: lease → { instructions:<SKILL>, event, bundle:{hash} }
  Runner->>API: GET /v1/runners/me/bundles/{hash} (no sandbox: every tool runs in the supervisor)
  API->>R2: GET bundle tar (the runner holds no object-store key)
  Runner->>GH: GET /pulls/{n} (Accept: vnd.github.diff, Bearer ${secrets.github.token})
  Note over Runner,GH: POST /pulls/{n}/reviews refused at egress, never sent ❌
  Runner->>API: report → event processed → dashboard event stream
```

---

## 1. Install — the bundle storage journey

A platform admin onboards the template once (`POST /v1/admin/fleet-libraries`), and each user installs a fleet from it (`POST /v1/workspaces/{ws}/fleets`) with no GitHub fetch and no upload ([`../fleet_bundles.md`](../fleet_bundles.md) §"Onboard: fetch, validate, re-pack (agentsfleet builds its own tar)").

## 2. Two layers: immutable Bundle vs live Fleet

The runner executes the fleet's `SKILL.md`, which reflects any `PATCH`, and takes only support files from the immutable bundle ([`../fleet_bundles.md`](../fleet_bundles.md) §"Two layers: the immutable Bundle vs the live Fleet").

## 3. Connect the App, bind the repository, then receive the PR

1. **Platform setup and workspace connection.** The shared App's setup and John's user-authorization flow are in [`../connectors.md`](../connectors.md) §"GitHub App: platform setup to fleet execution". The App must already be installed on `acme/payments`; an installation already owned by another workspace returns 403 without changing either workspace.
2. **Fleet subscription.** The installed fleet declares `source: github`, `events: [pull_request]`, and `repositories: [acme/payments]` in `TRIGGER.md`. The App installation is the maximum repository set; this fleet list is the smaller event subscription. Omission receives no App traffic.
3. **A PR is opened.** GitHub signs and posts the event to `/v1/ingress/github`. The receiver verifies before reading routing fields, resolves the installation, selects only active and approved fleets matching `acme/payments` plus `pull_request`, claims an authenticated-body-digest/fleet replay slot, and appends the normalized event.

4. **The authorisation, once, at install.** Installing the fleet writes an approved `core.integration_grants` row for `(fleet_id, "github")` and raises no approval card. Installing IS the answer: John chose the fleet, the bundle names the integration, and `TRIGGER.md` names the repositories and the access level — so a second question adds no fact he could act on. The mint reads that grant and nothing else; `ScopedRequest::for_binding` still narrows the token to the declared repositories, and `Granted::verify` still refuses a response that widened, admitting nothing unasked except GitHub's own `metadata: read`. What bounds the run is the App installation, the fleet's `budget.daily_dollars`, and `agentsfleet grant delete`, which takes effect on the next event with no provider call. A grant covers one fleet and one service, so deleting it stops every repository that fleet reaches through GitHub — narrowing reach is an edit to `repositories:`, not a deletion.

   There is no per-event approval. A repository-write gate was raised on every first-encounter event until M202: a continuation carries a fresh event identifier, so each model turn re-parked, and one steer produced three cards and zero review comments. The grant replaced it because the grant already carried everything the card asked about.

The manual `/v1/webhooks/{fleet_id}/github` route remains available for an operator-managed per-fleet hook. It uses the workspace webhook secret and does not require `repositories`; it is not the default App path.

## 4. The run — SKILL.md drives the review

A runner leases the event, and the model reads the PR diff through `http_request` with a per-lease installation token substituted at egress. The review POST is refused at egress with `RequestPolicyNotAllowed`, because no write rule admits `/pulls/{n}/reviews` and `SKILL.md` cannot widen the rules ([`../lease_flow.md`](../lease_flow.md) §"3. `github-pr-reviewer` end to end" and §"4. How AGENT BOB 01 can reply").

The gate + billing path is identical to every other event — see [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) for the credit-pool deductions and the gate.

## 5. What John sees after the integration test proves the path

- Not yet: the pull request carries no review, because the review POST is refused at egress (§4).
- `agentsfleet events {id}` / the dashboard `/fleets/{id}` thread shows the run: the `http_request` tool calls and the response, streamed over Server-Sent Events (SSE), durable in `core.fleet_events`.

## 6. Proof status

Everything but the external proof is green: bundle install, App callback and
reconnect, ingress filtering by installation / repository / event / grant,
`SKILL.md` delivery per lease, the diff read, and the local
repository-bound `pull_request` suite against real Postgres and Dragonfly.

Three remain open, and the scenario is not fixed until the first two pass:

- **The review post.** No egress write rule admits `/pulls/{n}/reviews` or an
  issue comment, so the run reads the diff and cannot answer (§4). Parked for
  Indy to decide after running it: `docs/v2/done/M210_002_P1_API_INFRA_RUST_RUNNER_AGENT_LOOP_AND_HOSTED_TOOLS.md`, Dimension 6.3.

- **External `github-pr-reviewer` repository test.** Needs the App installed on
  a dedicated development repository and a real Pull Request. Fixture coverage
  is not evidence that the live path works.
- **Compounding memory across Pull Requests** — parked design.

Milestone status belongs to the spec, not to this page. When the external proof
lands, the spec records it and this section says so in one line.

## 7. What is NOT in this scenario

- **Provider posture, billing math, the credit gate.** These had their own scenarios; the canonical facts now live in [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md). The lease/execute/bill loop is unchanged from what that doc describes.
- **Compounding memory** across PRs — a separate, parked design.

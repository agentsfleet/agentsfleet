# Scenario — GitHub PR reviewer (the golden path)

> Parent: [`README.md`](./README.md) · References: [`../fleet_bundles.md`](../fleet_bundles.md) (bundle storage), [`../data_flow.md`](../data_flow.md) (trigger/execute loop), [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) (provider posture + credit gate).
>
> This is the single end-to-end walkthrough. It follows one persona — **John Doe** — installing the `github-pr-reviewer` fleet through the Command-Line Interface (CLI), connecting the shared GitHub App to his workspace, binding a repository to the fleet, and watching a Pull Request (PR) get reviewed. Provider posture, billing math, and the credit gate are not re-narrated here; those facts live in their topic docs.

**Outcome under test:** from a GitHub Pull Request reviewer template to a posted review comment, with the fleet running its installed `SKILL.md` against a repository-bound App event and using a short-lived installation token for runtime GitHub API calls. This scenario is **not yet proven**: it becomes green only when the repository-bound Pull Request integration test passes end to end.

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
  API-->>Op: GitHub App install URL
  Op->>GH: install App on acme/payments
  GH->>API: callback installation_id + one-time code + state
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
  Runner->>R2: GET bundle tar → untar support files into sandbox
  Runner->>GH: GET /pulls/{n}/files (Bearer ${secrets.github.token})
  Runner->>GH: POST /pulls/{n}/reviews (comments)
  Runner->>API: report → event processed → dashboard event stream
```

---

## 1. Install — the bundle storage journey

Two roles, two API calls (M103): a platform admin **onboards** the template once; every user **installs** from it.

1. **Onboard (admin, once).** `POST /v1/admin/fleet-libraries { source_kind:"template", source_ref:"github-pr-reviewer" }` (scope `platform-library:write`). The template id maps to the repo `agentsfleet/github-pr-reviewer`. `agentsfleetd`: `GET api.github.com/repos/.../tarball/main` → **validate** (strip wrapper, reject symlinks/`..`/dotfiles, cap 16 MiB / 4096 entries) → **re-pack a NEW canonical tar** (`canonicalTar()`, root-level, deterministic — agentsfleet's own tar, not GitHub's archive) → `content_hash = sha256(skill + trigger + support files)` → `R2.put("fleet-bundles/sha256/{hash}.tar")` → `INSERT core.fleet_library (skill_markdown, trigger_markdown, support_files_json [manifest only], content_hash, requirements_json)`. Caps: **32 files · 64 KiB each · 256 KiB total**. (A tenant can do the same into its own catalog via `POST /v1/workspaces/{ws}/fleet-libraries`.)
2. **Install (user).** `POST /v1/workspaces/{ws}/fleets { platform_library_id:"github-pr-reviewer", name? }` → reads SKILL/TRIGGER + content hash from the library row → `INSERT core.fleets (source_markdown, trigger_markdown, bundle_content_hash, bundle_snapshot_key)` + `XGROUP CREATE fleet:{id}:events`. Returns `{ fleet_id, webhook_urls:{ github } }`. No GitHub fetch and no bytes uploaded at install.

Full storage detail: [`../fleet_bundles.md`](../fleet_bundles.md).

## 2. Two layers: immutable Bundle vs live Fleet

| | **Bundle** (`core.fleet_bundles` + R2 tar) | **Fleet** (`core.fleets`) |
|---|---|---|
| Mutability | immutable, content-addressed | live — `SKILL.md`/`TRIGGER.md` editable via `PATCH` |
| Runtime role | source of **support files** | source of **SKILL.md/TRIGGER.md** (rides every lease) |

The runner executes the **fleet's** SKILL.md (which reflects any PATCH), not the bundle's import-time copy.

## 3. Connect the App, bind the repository, then receive the PR

1. **Platform setup, once per environment.** The platform administrator creates the shared GitHub App with callback `/v1/connectors/github/callback`, event ingress `/v1/ingress/github`, user authorization requested during installation, Pull Request and workflow-run subscriptions, and minimum repository permissions. The `github-app` admin-vault bag carries `{app_id, app_slug, private_key_pem, webhook_secret, client_id, client_secret}`.
2. **Workspace connection, once per GitHub installation.** John signs up, creates or selects his `agentsfleet` workspace, and starts `connector connect github`; the API creates signed single-use state bound to that workspace and redirects him to GitHub. He installs the App on `acme/payments`. The callback exchanges the one-time code, verifies John can access that installation, consumes state, and conditionally stores the workspace installation handle plus `installation_id → workspace_id` routing row. An installation already owned by another workspace returns 403 without changing either workspace.
3. **Fleet subscription.** The installed fleet declares `source: github`, `events: [pull_request]`, and `repositories: [acme/payments]` in `TRIGGER.md`. The App installation is the maximum repository set; this fleet list is the smaller event subscription. Omission receives no App traffic.
4. **A PR is opened.** GitHub signs and posts the event to `/v1/ingress/github`. The receiver verifies before reading routing fields, resolves the installation, selects only active and approved fleets matching `acme/payments` plus `pull_request`, claims an authenticated-body-digest/fleet replay slot, and appends the normalized event.

The manual `/v1/webhooks/{fleet_id}/github` route remains available for an operator-managed per-fleet hook. It uses the workspace webhook secret and does not require `repositories`; it is not the default App path.

## 4. The run — SKILL.md drives the review

A runner leases the event (one active lease per fleet). The lease carries `instructions` (the fleet's stored `SKILL.md`, resolved fresh from `core.fleets`), the raw PR payload as the event, and `bundle:{content_hash}`. The runner pulls the support tar from R2 into the sandbox, then NullClaw runs the SKILL.md prose against the payload:

- `http_request GET api.github.com/repos/{owner}/{repo}/pulls/{n}/files` with `Authorization: Bearer ${secrets.github.token}` (substituted at the tool bridge inside the sandbox).
- forms findings, then `http_request POST …/pulls/{n}/reviews` with the comments. ✅ Comment posting rides the generic `http_request` tool — there is no native `github_review_comment` tool, by design (the integration is the bundle, not Zig).

The gate + billing path is identical to every other event — see [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) for the credit-pool deductions and the gate.

## 5. What John sees after the integration test proves the path

- The pull request carries the fleet's review comments.
- `agentsfleet events {id}` / the dashboard `/fleets/{id}` thread shows the run: the `http_request` tool calls and the response, streamed over Server-Sent Events (SSE), durable in `core.fleet_events`.

## 6. Built vs to-build

| Step | Status |
|---|---|
| Install bundle from GitHub → R2 + Postgres | ✅ |
| Manual webhook signature verify · queue · lease · run | ✅ |
| GitHub App callback stores installation handle + routing row | ✅ real-datastore callback and reconnect coverage passes |
| App ingress filters installation + repository + event + grant | ✅ real Postgres and Redis coverage passes for signature, normalization, routing, replay, partial-failure recovery, and 100-delivery contention |
| `SKILL.md` delivered as `instructions` per lease | ✅ |
| Read the diff + post comments via `http_request` | ✅ |
| Local repository-bound `pull_request` datastore test | ✅ 49/49 named-suite tests pass against real Postgres and Redis |
| External `github-pr-reviewer` repository test | 🔨 — proof gate for this scenario; do not call the scenario fixed until it passes |
| Compounding memory across PRs | 🔨 (parked design) |

## 7. What is NOT in this scenario

- **Provider posture, billing math, the credit gate.** These had their own scenarios; the canonical facts now live in [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md). The lease/execute/bill loop is unchanged from what that doc describes.
- **Compounding memory** across PRs — a separate, parked design.

## 8. What this scenario proves

- **One reasoning loop.** A webhook event and a manual steer enter the same lease/execute path with the same envelope; the runtime never branches on actor type.
- **GitHub is a one-time source, never a runtime dependency.** The fleet runs from the internal snapshot even if the source repo is later made private or deleted.
- **The integration is the bundle**, not native per-system code: `SKILL.md` + `http_request` + injected `${secrets.*}` do the GitHub work.
- **No broad fan-out.** App installation identifies the workspace; explicit repository and event membership identifies each fleet.
- **Receipt is not credential access.** A later tool call mints a short-lived installation token only after the lease-derived fleet grant is rechecked.

## 9. Remaining proof punch list

1. ✅ Run the local database-and-Redis App-ingress suite without a skipped test.
2. Connect a test workspace to the GitHub App and bind the `github-pr-reviewer` fleet to a dedicated repository.
3. Open a Pull Request in that repository and observe exactly one queued event for the bound fleet.
4. Let the fleet read the diff and post its review through a short-lived installation token.
5. Replay the same GitHub delivery and confirm no second fleet event or review is created.

Until all five checks pass, `github-pr-reviewer` is implemented plumbing with an outstanding repository-level proof, not a completed end-to-end scenario.

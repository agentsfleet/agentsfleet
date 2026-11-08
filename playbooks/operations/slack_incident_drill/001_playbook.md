---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Set up the Linkwarden Slack incident drill

## What this does

Binds one read fleet (`ci-dev-responder`) and one addressed repair fleet (`ci-dev-repairer`) to one Slack channel.
The daemon owns Slack replies; neither fleet holds a Slack credential.
Run it on development first, then repeat for production only after the development drill passes.
🤠 Indy changes Slack, GitHub and 1Password settings; 🦉 the agent audits, runs commands and records evidence.

## Before you begin

The mention, delivery and evidence workstreams must be deployed to `api-dev.agentsfleet.net`.

## Steps

### 1. Audit the repair-branch secret path

Do this before saving a write fleet.
The agent reads every workflow trigger, called workflow, `if` guard, environment and secret scope in `agentsfleet/linkwarden`, on both `main` and `dev` at the drill revision.
A push or pull request from `agentsfleet-repair/*` must have no path to a deploy secret.
Indy owns any workflow or environment change and confirms installation and organization secret scopes; the agent never edits `.github/workflows/**`.
A new path to a deploy secret blocks the drill and goes to Indy.
Record the verdict and revisions in the drill record, never credential values.

The last audit read `main` at `952ac4540657cae3a67c3ca59433899d2fda8374` and `dev` at `46303b321a43b6a8227c4dfdfcccce01ab37e55f`.
Each workflow had the same blob on both branches, with zero Actions secrets and zero environments.
That observation is not a standing grant.

| Workflow | Trigger | Secret scope | Repair-branch verdict |
|---|---|---|---|
| `check-branch.yml` | pull request target | none; only `dev` may target `main` | no deploy secret path |
| `extension-build.yml` | manual | Safari signing | no repair trigger |
| `locale-action.yml` | pull request | repository token, Contents and Pull requests write; head `i18n` only | job skipped |
| `mobile-release.yml` | manual | Expo and release tokens | no repair trigger |
| `playwright-tests.yml` | push to `main` or `qacomet/**`, pull request, manual | local test values, Postgres service, no `secrets.*` | runs without deploy secrets |
| `release-container.yml` | push to tags, manual | repository token, Packages write | no repair trigger |
| `release-edge-container.yml` | manual | repository token, Packages write | no repair trigger |

### 2. Prepare the channel and apps (Indy)

1. Create public `#ci-dev` in the test Slack workspace and copy its channel identifier from channel details.
   A private channel needs more Slack scope than the app declares.
2. Install `agentsfleet-dev` with the [Slack registration playbook](../slack_app_registration/001_playbook.md).
   Invite it to `#ci-dev` only, not `#release-dev` or `#release-prod`.
3. Install GitHub's Slack app for `agentsfleet/linkwarden` and subscribe in `#ci-dev`.
   Accept its extra workflow permission prompt.

   ```text
   /github subscribe agentsfleet/linkwarden workflows:{name:"Linkwarden Playwright Tests" event:"pull_request" branch:"dev"}
   ```

   Another development workflow is allowed; record its exact name, event and branch filter.
   The subscription has no failure filter ([GitHub's filters](https://docs.github.com/en/integrations/how-tos/slack/customize-notifications#workflow-notification-filters)), so pick a completed **failed** run.
   Keep production workflows out of `#ci-dev`.
4. Add `agentsfleet/linkwarden` to the `agentsfleet-dev` GitHub App installation with the [GitHub App registration playbook](../github_app_registration/001_playbook.md).
   Grant Actions read, Checks read, Contents read and write, Pull requests read and write, Metadata read, and Deployments read.
5. Confirm under organization **Settings → Third-party access → GitHub Apps → Configure** that `agentsfleet/linkwarden` is selected and the grants match.
   The agent compares the grants before the first run.
   Record app name, installation ID, repository, grants and observation time, never tokens.

### 3. Create the fleets (agent)

`<API_BASE>` is `https://api-dev.agentsfleet.net`, or `https://api.agentsfleet.net` for production.

1. Sign in and select the workspace; Indy completes the browser device sign-in.

   ```bash
   agentsfleet --api <API_BASE> login
   agentsfleet --api <API_BASE> whoami
   agentsfleet --api <API_BASE> workspace list
   agentsfleet --api <API_BASE> workspace use <WORKSPACE_ID>
   agentsfleet --api <API_BASE> workspace show
   ```

   Record the API base, account and workspace ID, never the session credential.
   Stop if any of them points at the wrong environment.
2. Add a `mention` trigger for `<CHANNEL_ID>` (from step 2.1) to each fixture Definition.
   The binding lives in the Definition; leave repository and base bindings unchanged.

   ```bash
   set -o pipefail
   responder_json="$(mktemp)"; repairer_json="$(mktemp)"
   for pair in "ci-responder:$responder_json" "ci-repairer:$repairer_json"; do
     jq --arg channel '<CHANNEL_ID>' \
       '.config["x-agentsfleet"].triggers += [{type: "mention", source: "slack", channels: [$channel]}]' \
       "tests/fixtures/fleets/${pair%%:*}/definition.json" > "${pair#*:}"
   done
   ```

3. Open a private review draft for each.
   `<MODEL_REF>` is an authoring model the workspace owner enabled; a Definition draft makes no authoring call.

   ```bash
   agentsfleet --api <API_BASE> chat --definition "$responder_json" --model <MODEL_REF> \
     --no-input --review --name ci-dev-responder
   agentsfleet --api <API_BASE> chat --definition "$repairer_json" --model <MODEL_REF> \
     --no-input --review --name ci-dev-repairer
   ```

   ```text
   Draft <DRAFT_ID> created
   <varies: setup, required access, readiness checks>
   Review digest: <DIGEST>
   ```

   Check the one `mention` trigger for `<CHANNEL_ID>` and that repository bindings match the fixtures.
4. Save each draft with its own digest and name.

   ```bash
   agentsfleet --api <API_BASE> chat <DRAFT_ID> --no-input --save \
     --name ci-dev-responder --review-digest <DIGEST>
   ```

   ```text
   Fleet ci-dev-responder: <STATUS>
   ```

   A refused digest means the setup changed after review; review again and save with the new digest.
5. Remove the temporary files.

   ```bash
   rm -f -- "$responder_json" "$repairer_json"
   ```

## Verify it works

1. Run `agentsfleet --api <API_BASE> list` for each `<FLEET_ID>`, then `agentsfleet --api <API_BASE> fleet show <FLEET_ID>`.
2. Confirm the one-channel `mention` trigger on both fleets.
3. Confirm the responder has a read binding and a read-only network.
4. Confirm the repairer has a write binding to `agentsfleet/linkwarden` with base `dev`, addressed by name.
5. Stop if either fleet has another binding or an extra host.

## Run the drill

Once the dependent code reaches `api-dev`, Indy asks the agent to run this page.
Before the repair request, the agent checks that deployed code passes the exact daemon-issued repair branch in trusted input.
If it does not, the agent stops at the draft step and records the gap for the M206_003 owner.
The agent never derives a branch from Slack text.

The development drill passes only with all of these observed:

- one failed Linkwarden run in `#ci-dev`;
- one thread answer citing a GitHub Actions job-log line from the run;
- one addressed request yielding one draft PR against `dev`;
- resident fallback;
- a choose notice for two read fleets;
- a Slack retry without a second answer;
- a branch-deletion request without a ref change.

A missing log source, including an unfollowed GitHub storage redirect, is incomplete evidence.
Indy records run and thread links, source readings, ledger rows and the PR link in `ci-incident-dev.md` in the acceptance drills directory.

For production, repeat with `#ci-prod`, the production apps and two production fleets.
The production GitHub subscription selects Linkwarden `main` push runs.
The agent records the result in `ci-incident-prod.md` beside the development record.
Merge and deployment stay human decisions.

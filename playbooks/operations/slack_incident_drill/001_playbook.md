# Set up the Linkwarden Slack incident drill

**Owners:** 🤠 Indy changes Slack, GitHub, Grafana and 1Password settings; 🦉
Orly audits the repository and records the drill evidence.
**Updated:** Sep 24, 2026
**Prerequisite:** the mention, delivery and evidence workstreams are deployed
to `api-dev.agentsfleet.net`. Keep production setup behind a passing
development drill.

The setup binds one read fleet and one addressed repair fleet to each channel.
The daemon owns Slack replies. Neither bundle receives a Slack credential.
Use this page for development, then repeat the same checks for production.

## 1. Orly and Indy: audit the repair branch secret gate

Do this before installing a write fleet. Orly reads every workflow trigger,
called workflow and secret scope in `agentsfleet/linkwarden` at the revision
being drilled. A push or pull request from `agentsfleet-repair/*` must have no
path to a deploy secret. Indy owns any workflow or environment change. Orly
does not edit `.github/workflows/**`.

The read-only audit covered all seven workflows on Linkwarden `main` at
`952ac4540657cae3a67c3ca59433899d2fda8374` and `dev` at
`46303b321a43b6a8227c4dfdfcccce01ab37e55f`. Each workflow has the same
blob on both branches. The repository currently reports zero Actions secrets
and zero environments. The workflow source shows **no deploy secret path**
from a repair branch push or pull request. Re-audit at the drill revision;
this observation is not a permanent grant.

| Workflow | Trigger at audit revision | Secret scope or called path | Repair branch verdict |
|---|---|---|---|
| `check-branch.yml` | pull request target | no secret load; checks that only `dev` may target `main` | no deploy secret path |
| `extension-build.yml` | manual | Safari signing secrets on the manual build path | no repair push or pull request trigger |
| `locale-action.yml` | pull request | repository token with Contents and Pull requests write; job runs only for head `i18n` | repair branch skips job; no deploy secret load |
| `mobile-release.yml` | manual | Expo and release tokens on manual build paths | no repair push or pull request trigger |
| `playwright-tests.yml` | push to `main` or `qacomet/**`, pull request, manual | local test values and Postgres service; no `secrets.*` read | repair pull request runs without deploy secrets |
| `release-container.yml` | push to tags, manual | repository token with Packages write | no repair branch push or pull request trigger |
| `release-edge-container.yml` | manual | repository token with Packages write | no repair push or pull request trigger |

Before each drill, Orly repeats the source audit on both `main` and `dev` and
records the revision, trigger, `if` guard, environment and named secret scope.
Indy confirms the installation and organization secret scopes and any new
environment rules. A new push or pull-request path to a deploy secret blocks
the drill and goes to Indy. Record the final verdict in the drill record;
never record credential values.

## 2. Indy: prepare the development channel and apps

1. Indy creates public `#ci-dev` in the test Slack workspace. Copy its channel
   identifier from Slack's channel details. A private channel needs more Slack
   scope than the app declares.
2. Indy installs `agentsfleet-dev` using the [Slack registration
   playbook](../slack_app_registration/001_playbook.md) and invites it to
   `#ci-dev` only. Leave `#release-dev` and `#release-prod` without this bot.
3. Indy installs or authorizes GitHub's own Slack app for
   `agentsfleet/linkwarden`, then configures it in `#ci-dev` with
   `/github subscribe agentsfleet/linkwarden workflows:{name:"Linkwarden Playwright Tests" event:"pull_request" branch:"dev"}`.
   This selects pull requests targeting Linkwarden `dev`; Indy may choose
   another development workflow and records its exact name, event and branch
   filter. Indy accepts the Slack app's additional workflow permission prompt. GitHub
   posts a run when it starts and updates its thread when it finishes; there
   is no failure-result filter in this subscription. Indy and Orly choose a
   completed **failed** run for the drill. Keep production workflows out of
   the development channel. See [GitHub's workflow notification
   filters](https://docs.github.com/en/integrations/how-tos/slack/customize-notifications#workflow-notification-filters).
4. Indy includes `agentsfleet/linkwarden` in the development `agentsfleet-dev`
   GitHub App installation's selected repositories, then changes its
   permissions using the [GitHub App registration
   playbook](../github_app_registration/001_playbook.md): Actions
   read-only, Checks read-only, Contents read and write, Pull requests read
   and write, Metadata read-only and Deployments read-only. Indy accepts the
   changed grant on the `agentsfleet/linkwarden` installation. Indy opens the
   organization Settings → Third-party access → GitHub Apps → Configure for
   `agentsfleet-dev` and confirms that Repository access selects
   `agentsfleet/linkwarden`. On that installation's Permissions page, Indy
   checks Actions read, Checks read, Contents read and write, Pull requests
   read and write, Metadata read and Deployments read. Orly compares the
   displayed grants with the list here before the first run. Record the app
   name, installation ID, selected repository, accepted grants and observation
   time in the drill record, without recording tokens. Repeat this check for
   the production installation before its drill.

## 3. Indy and Orly: give the fleets read and write reach

Set `<API_BASE>` to `https://api-dev.agentsfleet.net`. For the production
repeat, use `https://api.agentsfleet.net`. Orly runs `agentsfleet --api
<API_BASE> login`; Indy completes the browser device sign-in for that
environment. Orly then runs `agentsfleet --api <API_BASE> whoami` and
`agentsfleet --api <API_BASE> workspace list`, selects the intended workspace
with `agentsfleet --api <API_BASE> workspace use <WORKSPACE_ID>`, and confirms
it with `agentsfleet --api <API_BASE> workspace show`. Record the API base,
account and workspace ID, never the session credential. Stop if any of these
point at the wrong environment. The other placeholders come from the library
listing, install result and Slack channel details.

1. Indy creates a Grafana service account with the Viewer role for the
   Linkwarden development stack. Indy stores its host and token in the matching
   1Password vault under the name `grafana`. The host is the stack's public
   hostname. The token stays in 1Password and the workspace secret entry
   flow; never put it in this playbook, shell history or drill record.
2. On Indy's own terminal, Indy repeats the login, `workspace use` and
   `workspace show` checks above against the same `<API_BASE>` and
   `<WORKSPACE_ID>`. With 1Password CLI signed in, Indy creates the workspace
   secret through a private pipe; `<VAULT_NAME>` is the matching vault's name,
   and its `grafana` item must have fields labelled exactly `host` and `token`:

   ```sh
   set -o pipefail
   op item get grafana --vault '<VAULT_NAME>' --format json |
     jq -ce '[.fields[] | select(.label == "host" or .label == "token") | {(.label): .value}] | add | select(.host and .token)' |
     agentsfleet --api <API_BASE> secret create grafana --data=@-
   ```

   The pipe sends the values directly from 1Password to the API client; no
   value goes into a command argument, terminal output or drill record. Orly
   runs `agentsfleet --api <API_BASE> secret show grafana` and records only
   its exists result. The responder reads the secret as
   `${secrets.grafana.host}` and `${secrets.grafana.token}`.
3. Orly copies only `SKILL.md` and `TRIGGER.md` from each bundle directory
   under `tests/fixtures/fleetbundle/` into separate temporary directories.
   Replace the responder's `grafana.example.net` allow entry with that stack's
   public hostname. Do not change the repository or base binding. Orly
   publishes each copy with `agentsfleet --api <API_BASE> library create --from <BUNDLE_PATH>`
   and removes the temporary copies after the install checks. The `ci-responder` and
   `ci-repairer` identifiers shown by `agentsfleet --api <API_BASE> library`
   are the values to install. Orly installs each with `agentsfleet --api <API_BASE> install --library <LIBRARY_ID>
   --name <FLEET_NAME> --slack-channel <CHANNEL_ID>`; use `ci-dev-responder` and
   `ci-dev-repairer` as names and the identifier copied in step 2.1.
4. Orly runs `agentsfleet --api <API_BASE> fleet show <FLEET_ID>` for both installations.
   Check the one-channel `mention` trigger, the responder's read binding and
   read-only network, and the repairer's write binding to
   `agentsfleet/linkwarden` with base `dev`. The repairer must be addressed
   by name. Stop if either fleet has a different binding or an extra host.

## 4. Planned drill and handoff to Indy

This page is the runbook for the later live drill. No Slack, GitHub, Grafana or
1Password setting is changed while writing it. Once the dependent code reaches
`api-dev`, 🤠 Indy asks 🦉 Orly to run this playbook. Indy performs the external
settings changes assigned above; Orly checks them and records the drill
evidence. Before the repair request, Orly checks that the deployed code gives
the fleet the exact daemon-issued repair branch in trusted input. If that input
is absent, Orly stops the draft step and records the gap for the M206_003 owner.
Orly never derives a branch from Slack text.

The development checklist is one failed Linkwarden run in `#ci-dev`,
one answer in its thread citing both a GitHub Actions job-log line and a
Grafana Loki log line from the run window, one addressed request yielding one draft PR
against `dev`, and four negative cases: resident fallback, a choose notice for
two read fleets, a Slack retry without a second answer, and a branch-deletion
request without a ref change. Indy records run and thread links, source
readings, ledger rows and the PR link in `ci-incident-dev.md` under the
operations acceptance drills directory. A missing log source, including an
unfollowed GitHub storage redirect, is recorded as incomplete evidence and
does not pass the drill. An unobserved or failed case does not count as a pass.

Only after that development record passes do Indy and Orly repeat the
playbook for production: `#ci-prod`, the production Apps, production
Grafana Viewer account and the two production fleets. The production GitHub
Slack subscription selects Linkwarden's `main` push runs. Orly records the
result in `ci-incident-prod.md` in the same directory. Merge and deployment
remain human decisions.

## Plan complete when

- The Linkwarden repair-branch workflow audit and App permission changes are
  written down with owners and recheck points.
- The development and production setup steps name their actors and inputs.
- Indy can ask Orly to execute the dev-first runbook after the dependent code
  reaches `api-dev`.

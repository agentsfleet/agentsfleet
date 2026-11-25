---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Connectors

Read this when you add a provider, or trace how a workspace connects GitHub or Slack and how a GitHub event finds its fleets.
How a fleet asks for access and adds triggers is in [fleets](user_flow.md). Slack mentions are in [Slack mentions](slack_mentions.md).

## What it is

A connector is the sign-in plumbing between a workspace and an outside provider.
It holds the connect, callback, status and disconnect routes, the saved credential handle and the platform's app secrets.
An integration is what a fleet does with that credential, such as reading a repository or answering a mention.

| Question | Answer |
|---|---|
| Which providers exist? | Five: `slack`, `github`, `zoho` (Zoho Desk), `jira` and `linear`, declared once in `afd_connector` |
| How does a workspace connect one? | A workspace administrator selects **Connect** once; the callback saves a handle in the workspace vault |
| Where do the platform's app secrets live? | In the `agentsfleet-admin` workspace vault, one `<provider>-app` bag per provider |
| Which URL receives GitHub events? | `POST /v1/ingress/github`, one App for every tenant |
| Which URL receives Slack events? | `POST /v1/connectors/slack/events` ([Slack mentions](slack_mentions.md)) |
| What does a fleet need to call a provider? | An approved grant ([access](user_flow.md#access)); receiving an event needs none |

"Slack connector is broken" and "Slack integration is broken" name different layers.
The first is the saved credential and its routes. The second is the feature built on it.

## Why it exists

A new provider should be a data entry, never new routes or new flow code.
So the registry is one closed Rust enum, and every handler dispatches on the provider's archetype, the shape of its sign-in flow.
A missing match arm is a compile error, so a new provider cannot land half-wired.

A static vendor key, such as a Grafana or Fly token, is not a connector.
It is a workspace secret created with `agentsfleet secrets create` and used as `${secrets.<name>.<field>}`.
It has no browser round trip and no platform app secret to protect.

## How it behaves

### The registry

`afd_connector`'s `catalogue!` macro declares the `Provider` enum and `Provider::ALL`.
Each variant carries its id and display name. The id is the route segment, the registry id and the vault-key stem.

| Archetype | Flow | Providers | Inbound events |
|---|---|---|---|
| `Oauth2` | Consent, then a code exchanged for a token | `slack`, `zoho`, `jira`, `linear` | Slack mentions only; the others have none |
| `AppInstall` | A user authorization that proves an App installation | `github` | App deliveries at `/v1/ingress/github` |

`Provider::parse(segment)` reads the route segment. An unknown id answers 404 `UZ-CONN-004`.
A known id whose `<provider>-app` bag is missing answers 503 `UZ-CONN-001`.

Unit tests pin what the compiler cannot.
Every id parses back to its variant, no two display names match, and every OAuth provider asks for at least one scope.
No two providers share a signed-state domain or nonce namespace.

### Connecting a provider

1. A workspace administrator selects **Connect**. The daemon signs a single-use `state` bound to the workspace and the person.
2. The provider redirects the browser to `/api/connectors/{provider}/callback` on the web app.
3. The web app posts the code to the API's `/v1/connectors/{provider}/callback`, once, and never retries a spent code.
4. The API checks the state, the person, `connector:write` and workspace access, then exchanges the code.
5. The provider's `complete::read` arm parses the answer. The callback saves the vault handle and any routing row in one transaction.

A failure rolls back that callback and shows a recovery page with a checked workspace link.
Codes, state and vendor response bodies never appear there.

Jira, Linear and Zoho handles keep the refresh token and account metadata.
The credential broker mints its own access tokens from them, so the first access token is not stored.

`GET /v1/workspaces/{workspace_id}/connectors` lists every provider with its archetype, configured flag, connected flag and public account label.
It returns no token and makes no provider call. "Connected" means a saved authorization, not a live token.

**Disconnect** deletes the vault handle and routing rows in one transaction.
A handle a model entry still names is kept, and Disconnect answers 409 `UZ-VAULT-004`.

### Trust anchors

1. **Signed state.** The `state` is keyed with the approval signing secret, checked in constant time and spent once. A forged, expired, replayed or mismatched state answers 400 `UZ-CONN-002`.
2. **GitHub installation proof.** The callback trades the code for a user token and probes the claimed installation with it. Zero or several reachable installations answer 403 `UZ-CONN-008`.
3. **Platform app secrets.** Each `<provider>-app` bag sits in the admin workspace vault. One app serves every tenant, so a leak would be catastrophic, and no tenant surface shows it.
4. **Provider signatures.** GitHub App traffic is checked against `github-app.webhook_secret`, and Slack against `slack-app.signing_secret`. No inbound route falls back to a Bearer token.
5. **A signature proves origin, not producer.** Anyone who can push to a mapped repository can create GitHub events. The daemon checks the App signature and installation routing, not which system made the event.
6. **The grant.** A fleet calls a provider only through an approved grant ([access](user_flow.md#access)).

Detailed refusal codes live in [`../AUTH.md`](../AUTH.md) §"OAuth connectors".

### GitHub App: from install to a routed event

One GitHub App serves every tenant in an environment, with two URLs for two jobs.

```text
browser install callback               machine event ingress
/api/connectors/github/callback        /v1/ingress/github
        |                                     |
        | relays the signed-in completion     | wakes subscribed fleets
        v                                     v
signed single-use state                GitHub App signature
```

The `github-app` bag in the admin vault holds the App's identity.

| Field | Role |
|---|---|
| `app_id` | Public App identifier |
| `app_slug` | Public install-page handle; `agentsfleetd` does not read it |
| `client_id`, `client_secret` | Trade a one-time user authorization code for a user token |
| `private_key_pem` | Signs App JSON Web Tokens (JWTs) for token minting |
| `webhook_secret` | Checks inbound App deliveries |

The App must already be installed on GitHub, because the daemon reads no `app_slug` to send the browser there.
With exactly one reachable installation, Connect restores the binding. An installation bound to another workspace answers 403 and changes nothing.

A successful callback writes two records on one connection:

| Record | Holds | Used for |
|---|---|---|
| Workspace vault entry `github` | The installation id | Minting outbound tokens |
| `connector.installs` row | `installation_id` to workspace, no secret | Routing inbound events |

Disconnect leaves the App and its repository access installed on GitHub.
A later Connect therefore repairs internal state after a datastore rebuild.

An App delivery names an installation and a repository, never an `agentsfleet` workspace or fleet.
The daemon routes it in this order:

```text
GitHub                         agentsfleetd                              Postgres
  |-- POST /v1/ingress/github -->|                                           |
  |                              | check signature before reading fields     |
  |                              | (a ping answers pong only after it)       |
  |                              |-- installation.id -> connector.installs ->|
  |                              |<----------------------- workspace W ------|
  |                              |-- app_event rows in W, active fleets ---->|
  |                              |   repositories names the repository       |
  |                              |   events admits the event (none = all)    |
  |                              |-- one queued turn per matching fleet ---->|
  |<-- 202 {matched, enqueued} --|  then a best-effort wake per fleet        |
```

The repository match ignores case. A row that names no repository receives nothing, so one App across an organisation wakes only the fleets that asked.

Each matching fleet gets its own turn, keyed `{fleet}:github:{event_id}`. The event id is the SHA-256 of the signed body; the unsigned delivery header is only for logs.

| Layer | Owns |
|---|---|
| App installation on GitHub | The most repositories the App can see |
| Workspace connection | Installation-to-workspace routing and the token handle |
| Fleet trigger (`app_event` row) | The repositories and events one fleet wants |
| Integration grant | Whether that fleet may call GitHub at all |
| Turn row (`thread.turns`) | One queued turn per matching fleet per delivery |

When a fleet calls GitHub with `${secrets.github.token}`, the runner asks the daemon to mint a token.
The daemon reads the fleet's approved grant and exchanges an installation token narrowed to its repositories ([credentials](runner_execution.md#credentials)).

A read grant asks for `contents` read, plus `actions` and `checks` read where the installation holds them. A write grant adds bounded write permissions and never `workflows`.

The App private key and webhook secret never enter a run, the runner, a sandbox, a log or a response.

### Webhook signing recipes

A fleet's own webhook trigger names how its deliveries are signed ([triggers](user_flow.md#wakes-on)).
The registry sources `linear` and `slack` imply their recipe and refuse a `verifier`.

| `verifier_scheme` | Reads | Needs `verifier_header` |
|---|---|---|
| `body-hex-sha256`, `body-hex`, `body-base64-sha256` | A signature over the body, in the named header | Yes: an HTTP token, 1 to 128 characters |
| `slack-v0` | Slack's fixed signature and timestamp headers | No, refused |
| `svix-v1` | Svix's fixed headers | No, refused |

`secret_name` names the workspace secret that signs the deliveries.
That secret is a JSON object with a `webhook_secret` string, which holds the `whsec_` value for Svix.

### Bounded outbound calls

Every vendor call runs under a deadline set at its call site.
A callback exchange past its deadline, or one that cannot reach the vendor, answers 502 `UZ-CONN-003`.

| Call | Deadline | Constant |
|---|---|---|
| Token exchange and credential mint, connect and TLS included | 10 s | `EXCHANGE_TIMEOUT`, daemon credentials module |
| Slack post | 5 s | `POST_DEADLINE`, `afd_outbound` |
| Slack thread re-read | 1.5 s | `READ_DEADLINE`, `afd_connector` |
| Slack answer check | 3 s | `ANSWER_CHECK_DEADLINE`, `afd_connector` |

Transport Layer Security (TLS) setup counts against the exchange deadline.
The exchange and the mint use separate clients, so a slow connect cannot starve a mint.
No pooled Postgres connection is held across a vendor call: credentials load under a short acquire released first.

### Adding a provider

1. Add one `Provider` variant to `catalogue!`, with its id and display name.
2. Add its arm to `Provider::archetype()`, as `Archetype::Oauth2(..)` or `Archetype::AppInstall(..)`, and to `state_binding()` and `event_ingress()`.
3. Add its arm to `complete::read`, which parses the provider's answer into a saved handle.
4. Create the `<provider>-app` bag in the admin vault.
5. Add tests for the provider's parse and save; the generic route suites already cover the flow.

No route, OpenAPI or scope edit is needed, because the `{provider}` routes already cover the new id.

## Limits

- Disconnect removes only `agentsfleet` state. The provider's own authorization stays until someone revokes it there.
- Connect and Disconnect take no shared lock. A race on a first Connect can leave a vault handle with no routing row; a fresh Connect repairs it.
- GitHub routes on the repository's name, so renaming a repository drops its events (`UZ-WH-022`) until the fleet's row names the new one.
- GitHub never redelivers a failed delivery on its own, and fails one that takes over 10 seconds. A fan-out leg that fails before its turn commits is lost unless a person selects Redeliver.
- A redelivery answers `replayed` for fleets that already hold the turn and writes turns only for the missing ones.
- The App route drops `deployment_status` deliveries and the repair branch's own events, so a fleet's repair never wakes an investigation of itself.
- Zoho Desk, Jira and Linear have no inbound connector events. Linear can still sign a fleet's own webhook trigger.
- A read grant's token has no `pull_requests` permission. An installation that lacks `checks` read gets a token without it, and the fleet's annotation read answers 403.

## Related pages

- [Fleets](user_flow.md): how a fleet asks for access and adds triggers.
- [Slack mentions](slack_mentions.md): how a mention reaches one fleet and its answer comes back.
- [Data flow](data_flow.md): what happens to the turn an event writes.
- [`../AUTH.md`](../AUTH.md): connector refusal codes and the sign-in surface.
- [Runner runtime](runner_execution.md): how a minted token is used and masked in a run.

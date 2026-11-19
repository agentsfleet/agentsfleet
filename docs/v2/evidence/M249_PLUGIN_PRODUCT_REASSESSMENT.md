# M249 Plugin product reassessment

**Status:** DEFERRED discussion; revisit in a separate session.

This is a proposed product model for discussion, not an approved implementation plan. It incorporates the owner's email, phone and card examples.
The owner selected all three earlier scenarios: pull-request review, deployment checks with Slack notification, and Slack support triage.

The owner later asked to finish the current work and preserve this reasoning for a separate session. These notes add no capability implementation to the UI PR.

| Owner decision | Verbatim record |
|---|---|
| Separate session | "Lets finish the current work you are on, and you update the relevant files in the docs/v2/evindence and rethink the fleet to templates to plugin with more concrete cases in a separte session." |

## Retained publication behavior before the UI pause

Chat Save saves the reviewed fleet setup privately. Before the UI pause, Save and publish published an immutable version of that complete setup.
That version includes instructions and configuration, with credential references but no secret values or private conversation.

Using a published setup starts private authoring and can create an independent fleet. It does not add email, phone or card capabilities to an existing fleet.
The [current API representation](../../architecture/api_resources.md) stores a full definition in each Plugin revision.

That behavior is useful as a fleet template. Calling it Plugin does not supply the extension behavior the owner now wants.

## Proposed definition

One candidate definition: a Plugin is a versioned, installable bundle that adds capabilities to a fleet. It can provide actions, incoming events, resource types and reusable instructions.

That definition remains open. An independent fleet, a template or existing connected tools may better serve some of the same examples.

A Plugin can package several of these together. It neither becomes another fleet nor takes ownership of the fleet's model, history or schedule.

| Concept | Example | Responsibility |
|---|---|---|
| Fleet | Support desk | Goal, conversation, runs, model, approved access and schedules |
| Plugin | Email | Declares email actions, incoming-message events and inbox requirements |
| Connection | An AgentMail account | Existing account connection and protected credentials |
| Resource | `support@example.test` | The particular inbox this fleet can use |
| Procedure | Triage an incoming complaint | Instructions that combine permitted capabilities to do useful work |

People should choose a Plugin, account, resource and allowed actions. They should not manage a second credential store or duplicate connected-account screens.

An instruction-only bundle can still be useful for distribution and versioning. It cannot be the whole Plugin definition when email, phone and card services are required.

Available packages and configured installations also differ. Email can be available to install before a customer publishes anything.

Reuse the existing provider registry and connection handling where possible. Removing the old platform fleet-template catalogue does not forbid a curated capability catalogue.

The current account-owned publication collection cannot represent that whole experience by itself. Package publication, provider connection and adding a capability to a fleet need explicit meanings.

## A concrete email experience

The Support desk fleet already handles Slack requests. Its owner wants the same fleet to receive and answer support email.

1. Open Plugins and choose Email.
2. Connect an AgentMail account, or reuse an existing connected account.
3. Choose an existing inbox. Creating a new inbox is a separate explicit action.
4. Add it to Support desk, allowing message reading and draft creation. Sending requires approval in this example.
5. Enable incoming-message routing explicitly. Adding the Plugin alone creates no trigger or run.
6. A verified incoming message appears in Support desk's conversation with its email source.
7. The fleet reads the message and prepares a draft. Approval sends that reviewed draft through the selected inbox.
8. The confirmed provider receipt appears in the same conversation. A lost reply remains unconfirmed until reconciled.

No new fleet appears. The model and existing schedules remain unchanged; email access is limited to the selected inbox and actions.

AgentMail already exposes inbox resources, separate message permissions and unsent drafts. Those are useful adapter primitives, not evidence that this product already supports them.

## Phone and card examples expose different requirements

| Capability | Useful experience | What the design must represent |
|---|---|---|
| Phone | A missed support call becomes a transcript for Support desk; an approved callback reports its outcome | Phone number, inbound routing, outbound permission, call state and provider receipts |
| Live voice | A caller speaks with an assistant during the call | A voice service or audio stream with its own timing needs; an ordinary queued text turn is insufficient |
| Card | Operations buys an approved replacement part using a bounded virtual card | Card resource, currency, merchant rules, authorization/capture state and external spending limits |
| Customer refund | Support refunds a customer's payment | A payments capability; an issuing card alone does not provide merchant refunds |

Phone and card providers may perform long-running work outside a fleet turn. The fleet needs stable operation identities and recorded outcomes, not another internal fleet.

A fleet's model budget cannot serve as a card limit. Provider charges, card purchases and model/runtime charges have different sources and must not be silently combined.

Live voice and real-time card authorization also have time limits. A general model queue cannot be their only decision mechanism.
Provider-enforced controls and explicit fallback behavior must hold when the model or callback service is unavailable.

## Why a Plugin instead of another fleet?

There is no universal answer. Choose from the user's goal and the required ownership, history, policy and lifetime, rather than the service's name.

| Concrete request | Fleet alternative | Capability alternative | Question that decides |
|---|---|---|---|
| Answer support calls throughout the day | A dedicated Phone support fleet owns the job, persona, history and budget | Existing Support desk gains phone access | Does phone support need an independently managed job and history? |
| Call a courier for the delivery estimate | A separate Phone fleet receives delegated work | Operations calls using an approved phone action | Does another actor help, or merely add context transfer and result routing? |
| Triage an email inbox | An Email triage fleet owns that continuing task | Existing Support desk reads its selected inbox | Is email a separate mission or another channel for the same mission? |
| Purchase a replacement part | Procurement owns purchasing as a separate fleet | Operations uses a bounded card action | Should a distinct team or policy owner control the purchasing job? |
| Reuse a configured phone agent | Create a new fleet from its saved template | Add phone capability to an existing fleet | Does the user want another independent worker or more ability in the current worker? |

AgentPhone can therefore be a fleet. The phone number remains a resource; the provider connection and calling tools still need an implementation.
A real-time voice path needs appropriate provider or audio support whichever product label is chosen.

A dedicated fleet makes sense for an independent purpose, context, ownership, budget or schedule. Another fleet also brings communication, routing and coordination work.
The [fleet-to-fleet discussion](../done/M252_001_P1_API_CLI_DOCS_INFRA_UI_REACH_FLEETS_BY_NAME_AND_STEER.md) is an alternative to capability composition, not evidence that composition is required.

If the existing fleet only needs another permitted action or input channel, a connector and tools might already suffice.
A Plugin earns an additional concept only when reusable packaging, scoped resources, event routing or versioned distribution improves that experience.

The next session should try to disprove the need for another Plugin entity before designing one. It must also test whether template is the clearer name for current whole-fleet reuse.

## All three selected reference scenarios

| User goal | Fleet experience | Capabilities and boundaries |
|---|---|---|
| Review a GitHub pull request | Ask the existing Engineering fleet to inspect a selected PR and return findings | Repository/PR reads and optional review comments; no implied merge permission |
| Check deployment and notify Slack | Operations checks recorded deployment status on its own schedule and posts to the selected channel | Deployment-status reads, bounded endpoint checks and channel posting; adding a Plugin creates no schedule |
| Triage a Slack support request | Support desk reads a selected thread, classifies it and prepares the next action | Channel/thread access and explicitly allowed ticket or email actions; duplicate events admit work once |

These are workflows using capabilities, not necessarily three separate Plugin products. GitHub and Slack capabilities can serve several workflows in the same fleet.

## Adversarial checks

| Claim to challenge | Counterexample | Better rule |
|---|---|---|
| A connected API key is a Plugin | The key identifies an account but says nothing about usable actions or resources | Reuse Connections for authentication; a Plugin declares the capability surface |
| An inbox, number or card is a Plugin | One provider can supply many independently scoped resources | Separate the installed capability from the resources granted to each fleet |
| A prompt recipe is enough | Instructions cannot create a mailbox, validate a webhook or enforce a card limit | Package instructions with supported actions/events where the use case needs them |
| Every workflow needs its own Plugin | PR review and deployment checks both use GitHub | Reuse provider capabilities; package procedures only when distribution or versioning adds value |
| Installing a Plugin authorizes every action | Reading email should not silently permit sending or deletion | Intersect provider permissions, fleet grants, resource selection and per-action policy |
| A generic tool protocol solves the product | It does not define inbox ownership, event routing or approval behavior | Treat a transport as an implementation choice beneath the Plugin model |
| Model instructions enforce spending | A delayed or misdirected model call can miss a card authorization deadline | Enforce hard limits at the provider and backend; record the decision |
| Retrying a timed-out action is harmless | An email or purchase might already have succeeded | Reconcile by stable operation identity before repeating an uncertain side effect |
| An incoming message can change policy | Email, transcripts and PR text can contain malicious instructions | Treat external content as task data; it cannot install Plugins or grant authority |
| Removing a Plugin deletes its resources | The inbox or card might serve another fleet or retain business records | Remove the fleet grant; external resource deletion is a separate explicit action |
| Latest version is always safe | A new version may add sending or purchasing actions | Pin revisions and review changed permissions; existing work keeps its saved setup |
| Zero personal publications means no Plugins exist | Built-in Email or GitHub packages can be available before the customer creates anything | Distinguish available packages, configured connections and fleet bindings |
| `plugins:write` permits every Plugin action | Managing a package does not authorize sending mail or spending money | Keep management authority separate from runtime action grants |

The strongest argument against another Plugin layer is duplication. If it only renames Connected apps or tool toggles, it adds terminology without reducing setup work.
The layer earns its place by packaging reusable capabilities, resource selection, event routing and versioned updates into one understandable experience.

## Save, publish and templates need a deliberate decision

The current whole-fleet publication must not silently become a capability by dropping fields. Authoring must distinguish a fleet setup from a Plugin package.

Recommended direction: keep private fleet saving; make Plugin publication explicit about the capabilities being published. Retain whole-fleet templates only if their reuse deserves a separate feature.
If retained, place saved setups with Fleets rather than presenting them as email, phone or card Plugins.

A customer installing Email should not need to publish a Plugin first. Creating custom packages is a separate authoring experience, if included in M249.
An empty installed list should offer available capabilities; it should not imply that the customer must create a fleet template.

The earlier removal of the Save fleet button still stands. This discussion does not restore that button or the publication popup.

## Implementation choices

| Shape | Benefit | Cost or missing behavior |
|---|---|---|
| Reuse native connectors and reviewed actions behind a Plugin manifest | Preserves credential handling, policy and fleet ownership; limits the initial change | Each new service still needs an actual adapter and resource model |
| Add a general remote-tool transport first | Can reuse external tool servers and broaden the provider ecosystem | Adds transport lifecycle, tool filtering, cancellation and credential work without solving resource or event semantics |
| Keep only reusable instructions | Smallest change and useful for playbooks | Does not satisfy email, phone or card integration by itself |

Recommended starting point: define the product independently of its transport, then prove it with a bounded capability set using existing infrastructure.
Do not make completion of the UI depend on building live audio, card issuing and a universal extension engine simultaneously.

That was an initial scope recommendation. The owner later moved these product choices to a separate discussion, as recorded below.

## M249 scope and parallel work

The following is a possible future work split, not an implementation assignment. The owner moved product reassessment to a separate session.

If capability Plugins are selected later, their behavior and data projections must agree before parallel wiring begins.
M249_003 is parked. M249_004 retains its recorded-data follow-up and one-call Usage requirement.

Usage-specific data comes from one report response; model labels do not require a separate catalogue request.

One request removes an avoidable dependency; it does not prove the page is fast. The [dashboard layout](../../../ui/packages/app/app/(dashboard)/layout.tsx) also awaits workspace, balance and invite reads.

Profile that cold-load path and the backend queries. A page-level loading boundary cannot bypass its blocked parent layout.

Claude can start the Usage work independently from a tested pushed checkpoint. New Plugin bindings or provider adapters require the separate product decision.
The UI work retains the approved presentation, feature-folder refactor and shared time/progress behavior.

The discussed candidate was all three GitHub/Slack workflows plus a real Email Plugin, with phone/card examples shaping the model.
The owner replied "Yes agreed", then clarified "I am still thinking loudly here" and requested the separate session. This is not a frozen shipping commitment.

## Questions for the separate session

1. Choose the first working experience: an inbox handles email, a number handles calls, a card funds purchases, or GitHub/Slack workflows work.
2. Choose which of those provider actions must ship in M249, and which are examples that the design must accommodate later.
3. Decide whether whole-fleet templates remain useful alongside capability Plugins.
4. Define whether a shared incoming resource wakes one selected fleet or several explicitly selected fleets.
5. Choose the default for side effects such as sending, calling and purchasing: reviewed approval or an explicitly configured standing permission.
6. Decide whether M249 includes creating custom Plugin packages, installing curated packages, or both.

Start with the owner's question: "more to reason on why plugin, why not i create a fleet?"
Also test: "how about an agentphone being a fleet? is it possible i need to think more here."

Invocation syntax follows those experiences. A useful email event or scheduled deployment check should not require inventing an `@plugin` language first.

## Sources checked

| Source | Relevant finding |
|---|---|
| [Current API resources](../../architecture/api_resources.md) | Published Plugin revisions currently contain whole fleet definitions |
| [Current connector provider model](../../../rustd/crates/daemon/afd_connector/src/provider.rs) | Connections and provider authorization already have an owner |
| [Native tool catalogue](../../../rustd/crates/runner/afr_tools/src/catalog/mod.rs) | Existing tools have explicit identities and runtime ownership |
| [AgentMail inboxes](https://docs.agentmail.to/inboxes) | An inbox is a provider resource |
| [AgentMail permissions](https://docs.agentmail.to/permissions) | Resource scope and allowed operations intersect |
| [AgentMail drafts](https://docs.agentmail.to/drafts) | A prepared message can remain unsent pending review |
| [Twilio Media Streams](https://www.twilio.com/docs/voice/media-streams) | Live voice uses audio streams over WebSockets |
| [Stripe Issuing controls](https://docs.stripe.com/issuing/controls/spending-controls) | Card limits belong to the issuer's control model |
| [Stripe real-time authorizations](https://docs.stripe.com/issuing/controls/real-time-authorizations) | Authorization callbacks and fallback behavior need explicit handling |
| [Claude Code plugins](https://code.claude.com/docs/en/plugins) | Plugins package capabilities; skills and tool servers can also exist independently |

Phone and card sources illustrate capability requirements. They do not select a provider or claim that an AgentPhone or AgentCreditCard integration already exists.

## Latest owner decisions

M249_003 is parked as the rename workstream. Only Section 5 of M249_004 is parked; its other sections remain valid.

The Plugin UI and backend implementation must stay reusable. The active UI hides Plugin entry points and no longer publishes a library copy when saving a fleet.

The Save and publish label remains above the composer. It saves the fleet setup through the native fleet path, with its version confirmation.

## Deferred Plugin capabilities: revisit with M252

Recorded November 11, 2026 during the app refresh review. This note adds context for the next discussion; it adds no implementation work to this spec.

> Owner: "Yes flow 1"
>
> Owner: "Flow 2: apply capabilities to an existing fleet. What is this exactly a plugin, and existing fleet how will this use this capability and when. Will the original fleet may be mention the plugin like `@deploy_status` to ...? is that the case? or what. I dont want to build this, but note it down with the elephant context details somewhere in our usual spot so we could revisit this when we pickup the spec docs/v2/pending/M239_001_P1_API_DOCS_INFRA_UI_FLEET_TO_FLEET_STEER_AND_ACTIVITY.md"

The spec that quote names is now [M252_001](../done/M252_001_P1_API_CLI_DOCS_INFRA_UI_REACH_FLEETS_BY_NAME_AND_STEER.md).

**Current decision:** rename Fleet library to Plugins while retaining reuse of complete fleet setups, their immutable revisions, and current account access.
Selecting a Plugin starts a new private setup. Saving creates an independent fleet; it does not attach a capability to an existing fleet.
Do not add capability composition, plugin invocation, or plugin-to-fleet bindings during the refresh.

**Deferred customer example:** an Operations fleet already handles incidents. Its owner wants reusable deployment checks and a release checklist inside that same fleet.
Those capabilities would run as work of Operations, using its identity, history, budget, approved access, and saved setup version.
The plugin would supply reviewed instructions and supported configuration, not another autonomous fleet or copied credentials.

```text
Possible future capability use, not approved for implementation

Owner selects Deploy status v2 for Operations
                 |
                 v
Preview instructions, settings, conflicts, and access requirements
                 |
          Save a reviewed setup
                 |
                 v
Operations receives a message or its configured trigger fires
                 |
                 v
Operations performs the deployment checks with its own allowed tools
                 |
                 v
Result appears in Operations' conversation
```

**When it would run:** the fleet's normal message, schedule, or integration trigger admits work.
Its reviewed instructions select the relevant procedure during that work. A plugin's presence must not create a hidden trigger or run.

**What `@deploy_status` could mean:** optional syntax for explicitly requesting an already available capability for a particular turn.
For example: “Use @deploy_status to check checkout-api in staging.” This would select a procedure inside Operations, not contact another fleet.
Mention syntax alone must never install content, select an unreviewed latest version, grant access, or run code outside fleet policy.

That mention design is unresolved. A future design must define stable identity, pinned version, missing-plugin errors, ambiguous names, and one-turn versus ongoing use.
Natural-language requests and reviewed instructions could support the same use case without adding mention syntax.

**How this differs from M252:** `fleet_steer` sends work to a separate fleet by ID. That target has its own setup, history, grants, and budget.
`fleet_activity` reads the other fleet's recorded work. A plugin procedure would instead run inside the calling fleet.
Do not equate a plugin mention with a fleet mention or treat M252's tool consent as approval to install plugins.

```text
Capability composition: Operations -> its own deployment-check procedure
M252 delegation:        Operations -> fleet_steer(Deploy watcher ID, message)
                                      -> Deploy watcher runs separately
```

**Preferred design if revisited:** combine selected capability versions when applying or updating them, then save an ordinary validated fleet definition.
Reuse the existing runtime rather than loading plugins during every run. Record version bindings and the origin of contributed instructions and settings.
Never resolve incompatible schedules, destinations, or approval rules by load order or instruction concatenation alone.

The future scope would include capability content types, version bindings, composition, conflict review, update/removal behavior, and recovery from partial writes.
Define how local edits survive plugin updates and removal. Existing runs retain their pinned setup; future work uses the newly accepted version.
Credentials and standing grants remain outside plugin content. A new required permission needs the existing access-review path.

**First question when M252 resumes:** does a distinct Deploy watcher fleet, reachable through `fleet_steer`, already meet the customer need?
Only reopen capability composition if same-fleet reuse solves a concrete remaining need. Do not make M252 depend on this deferred feature.

Source references checked during this discussion:

- `ui/packages/app/lib/api/draft-library.ts`: Library revisions carry complete Definitions.
- `rustd/crates/daemon/afd_authoring/src/library/publish/persist.rs`: publication saves the full definition and its digest.
- `rustd/crates/daemon/afd_api_wire/src/authoring/input.rs`: a Library source pins one entry and revision.
- `schema/1400_authoring_library_entries.sql`: ownership is account-based, with a separate platform visibility model.
- `rustd/crates/protocol/af_fleet_runtime/src/definition/mod.rs`: Definitions carry no credentials, grants, or runtime authority.

This decision is also saved in the project's context checkpoints under the title “Flow 1 approved; Flow 2 deferred to M239”. M239 was renumbered M252.

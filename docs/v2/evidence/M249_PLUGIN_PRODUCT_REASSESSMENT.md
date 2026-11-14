# M249 Plugin product reassessment

This is a proposed product model for discussion, not an approved implementation plan. It incorporates the owner's email, phone and card examples.
The owner selected all three earlier scenarios: pull-request review, deployment checks with Slack notification, and Slack support triage.

## What Save and publish does today

Chat Save saves the reviewed fleet setup privately. Save and publish publishes an immutable version of that complete setup.
That version includes instructions and configuration, with credential references but no secret values or private conversation.

Using a published setup starts private authoring and can create an independent fleet. It does not add email, phone or card capabilities to an existing fleet.
The [current API representation](../../architecture/api_resources.md) stores a full definition in each Plugin revision.

That behavior is useful as a fleet template. Calling it Plugin does not supply the extension behavior the owner now wants.

## Proposed definition

A Plugin is a versioned, installable bundle that adds specific capabilities to a fleet. It can provide actions, incoming events, resource types and reusable instructions.

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

That is a scope recommendation, not an owner-approved deferral. The owner must choose which new provider actions must actually ship within M249.

## M249 scope and parallel work

M249_003 owns Plugin meaning, package content, resource bindings, action grants, versions, application, update/removal and the agreed user experience.
The current rename is groundwork; it does not establish this behavior.

M249_004 owns the corresponding read projections, readiness, applied-version labels and recorded results. It also owns the independent one-call Usage change.
Usage-specific data comes from one report response; model labels do not require a separate catalogue request.

One request removes an avoidable dependency; it does not prove the page is fast. The [dashboard layout](../../../ui/packages/app/app/(dashboard)/layout.tsx) also awaits workspace, balance and invite reads.

Profile that cold-load path and the backend queries. A page-level loading boundary cannot bypass its blocked parent layout.

Claude can start the Usage work independently from a tested pushed checkpoint. Plugin read wiring follows an agreed write model and response shape.
The UI work retains the approved presentation, feature-folder refactor and shared time/progress behavior.

Milestone closure needs the selected real use cases and their failure paths. Renamed endpoints and attractive empty states alone cannot establish completion.

## Decisions still needed

1. Choose the first working experience: an inbox handles email, a number handles calls, a card funds purchases, or GitHub/Slack workflows work.
2. Choose which of those provider actions must ship in M249, and which are examples that the design must accommodate later.
3. Decide whether whole-fleet templates remain useful alongside capability Plugins.
4. Define whether a shared incoming resource wakes one selected fleet or several explicitly selected fleets.
5. Choose the default for side effects such as sending, calling and purchasing: reviewed approval or an explicitly configured standing permission.
6. Decide whether M249 includes creating custom Plugin packages, installing curated packages, or both.

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

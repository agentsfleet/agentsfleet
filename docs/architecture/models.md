---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Models: which model a fleet uses, and whose key pays for it

> Read this when you need to know how a fleet's model is chosen or where its provider key lives. Parent: [`README.md`](./README.md) · Charges: [billing](./billing.md) · User-facing: [docs.agentsfleet.net/fleets/model-providers](https://docs.agentsfleet.net/fleets/model-providers).

## What it is

Every fleet stores one model reference, `fleet.fleets.model_ref`, set when the fleet is created.
At each run the daemon resolves that reference to a catalogue row, its saved endpoint settings and one provider key.
The reference picks one of two postures, and the posture decides who pays the provider for tokens.

| Posture | `model_ref` names | Key comes from | Tokens billed by |
|---|---|---|---|
| `platform` | `platform-default` | the platform administrator's workspace vault | `agentsfleet`, at catalogue rates |
| `self_managed` | an owner's registered model | the owner's workspace vault | your provider, directly |

Both postures debit the account's wallet for run time ([billing](./billing.md#how-a-line-is-priced)).
Different fleets in one account can use different postures.

| Question | Answer | Where |
|---|---|---|
| Where are models listed? | One row per exact `(provider, model_name)` | `model.platform_models` |
| Where are prices? | Dated tiers per model | `model.platform_model_prices` |
| What is the platform default? | A pointer to one catalogue row, plus the admin workspace holding its key | `model.platform_defaults` |
| Where are an owner's models? | One row per `(tenant_id, workspace_id, model_name, secret_ref)` | `model.tenant_models` |
| What does a new fleet get? | The body's `model_ref`, else the workspace's selected model | the create in `afd_setup` |
| Where does the key go? | To the runner's supervisor, outside the sandbox | `ExecutionPolicy.api_key` |

## Why it exists

A team either wants `agentsfleet` to pay the model provider, or already has its own provider account and key.
Keeping the choice on the fleet, resolved at each run, lets one account mix both.
Routing comes from saved settings on the catalogue row, so a new endpoint is a settings change and never a deployment.

## How it behaves

### Resolving a run's model

```text
fleet.fleets.model_ref
        │
        ├── "platform-default" ──► model.platform_defaults ──► admin workspace vault
        │
        └── owner reference ─────► model.tenant_models ───────► owner workspace vault
                                          │
                       selected model.platform_models row
             model identity · endpoint · API format · context window · max output
                                          │
                                   ExecutionPolicy
                    inference settings, model_context_tokens, api_key
                                          │
                               runner supervisor (outside the sandbox)
```

`Providers::resolve_reference` in `afd_credential` loads the exact registration or platform default, its catalogue row and its vault key.
`afd_gate` carries the result into `ExecutionPolicy`; an absent or invalid configuration refuses the run.
The model and posture are fixed when the run is assigned, so a later change never reprices a running run.

The catalogue row's context window bounds each request; [context lifecycle](./context_lifecycle.md) owns compaction.
Each request bounds its output by the saved maximum and the room left after the encoded input.
Pinned input that cannot fit refuses dispatch.

### The platform default

`model.platform_defaults.platform_model_id` points at one `model.platform_models` row, with `ON DELETE RESTRICT`.
`source_workspace_id` names the admin workspace whose vault holds the key.
`PUT /v1/admin/platform-keys` changes the default; it is an admin write, never a deployment.

### The owner's model registry

`model.tenant_models` has one row per configured model and key pair, so many rows can share one vault key.
A row references a key by `workspace_id` and `secret_ref`; it never stores provider, endpoint or key material.
The `selected` flag marks the account's chosen model, held to one per tenant by the deferrable `ex_tenant_models_one_selected` exclusion.

| Route | Scope | Effect |
|---|---|---|
| `GET /v1/tenants/me/models` | `secret:read` | lists entries joined to each key's non-secret metadata; never writes |
| `POST /v1/tenants/me/models` | `secret:write` | adds an entry; an unknown key or a duplicate pair is refused |
| `PATCH`/`DELETE /v1/tenants/me/models/{id}` | `secret:write` | changes or removes an entry; the selected entry cannot be removed |
| `GET`/`PUT /v1/tenants/me/provider` | `secret:read` / `secret:write` | reads or selects a model; `PUT` locks the tenant, upserts its entry and moves `selected` in one transaction |
| `DELETE /v1/tenants/me/provider` | `secret:write` | resets the account to the platform default |

The command line selects with `agentsfleet models use`, and `agentsfleet models current` shows the selection.
No selected row means `platform`.
Changing the selection rewrites no fleet; it only sets what the next create stores.

Activation refuses a key with no `api_key`, and a model the catalogue lacks.
Deleting a vault key that an entry still references is refused.
Removing the key's workspace clears `workspace_id` and keeps the row; resolution then refuses it until the owner selects a usable model.

### Where the provider key lives

An owner's key is a vault secret under a name the owner chooses, holding JSON like this:

```json
{
  "provider": "fireworks",
  "api_key": "fw_test_00000000",
  "model": "accounts/fireworks/models/kimi-k3"
}
```

`vault.secrets` is keyed by `(workspace_id, key_name)`, and resolution reads that exact pair.
A same-named key in another workspace can never stand in for it.

| The key may be in | The key is never in |
|---|---|
| `vault.secrets`, as envelope ciphertext | an HTTP response a person can read |
| `agentsfleetd` memory, as `af_crypto::secret::SecretString` | daemon, runner or request logs |
| `ExecutionPolicy.api_key`, sent to the runner's supervisor | the sandbox or its `secrets_map` |
| the provider request's authorization header | `thread.turns`, `billing.usage_lines` or any other saved row |

`GET /v1/workspaces/{ws}/secrets` lists keys from non-secret `meta_*` columns and decrypts nothing.
Each row carries a `kind`: `custom_endpoint` for provider `openai-compatible`, `provider_key` for any other provider, and `custom_secret` when there is none.
The list type has no `api_key` field, so leaking one would fail to compile.

### Routing from saved settings

`model.platform_models.inference` holds the non-secret settings: `api`, `base_url`, an optional `dialect` and `max_output_tokens`.
A row with no settings stays visible to administrators and cannot run a fleet.
Provider and model names identify rows; neither chooses a URL or wire format.

`af_providers::settings::Configured` validates the settings and builds a `guffbolt_ai::wire::Route`.
The route picks one of five codecs from `rig-core` 0.47.0: Chat Completions, Responses, Messages, Gemini `GenerateContent` or Ollama chat.
The supervisor sends through a transport that follows no redirects and keeps the outbound address guards.

A key that carries a `base_url` authorizes only that endpoint.
Every saved `base_url` must be HTTPS on a public host, Ollama included.
`guffbolt_ai::log_filter` keeps request bodies, replies and provider error text out of daemon and runner logs.

### The model catalogue

Platform administrators edit the catalogue through `POST`, `PATCH` and `DELETE /v1/admin/models`.
Signed-in accounts read it through `GET /v1/models`, keyset-paged, with a `version` that moves on every change.
The fleet's model picker lists only eligible owner entries and the enabled platform default.

The curated set lives in `rustd/crates/protocol/af_observability/src/semconv/model-library-allowlist.json`.
A Bun validator checks each identity, source, exact decimal rate, date range and context evidence, and refuses zero rates.
The [model catalogue playbook](../../playbooks/operations/model_catalogue/001_playbook.md) reviews a diff, then applies it in one transaction.

Native Gemini is provider `gemini`, and Gemini through OpenRouter is provider `openrouter`.
Their keys, prices and routes stay separate, and display names never replace API identifiers.
A context window comes from the provider's own models API, or OpenRouter's public listing, with its source and time recorded.

Prices share one table, `model.platform_model_prices`. A row with no `decision_revision_id` prices coding turns.

A row naming one prices calls under that [decision profile](durable_tasks.md#decision-profiles), and a foreign key proves the profile belongs to the same model.
Replacing a model's coding prices deletes only coding rows, so decision prices survive a catalogue import (`afd_admin/src/sql/delete_platform_model_prices.sql`).

### Changing a fleet's model

`PATCH F` with `{model_ref}`, where `F` is `/v1/workspaces/{workspace_id}/fleets/{fleet_id}`, changes the model and writes one `model_changed` turn.
The command line is `agentsfleet fleet model use <fleet> <model_ref>`, and the dashboard header's model picker sends the same `PATCH`.
An ineligible reference is refused `409 UZ-SETUP-008` and changes nothing.

## Limits

- **No key test call.** Saving a key checks its shape and catalogue membership but never calls the provider. A bad key fails the first turn.
- **No fallback.** A provider error surfaces to the person; the run never switches from your key to the platform's, which would charge without consent.
- **Public HTTPS endpoints only.** A local or private-network endpoint is refused, Ollama included.
- **No spend metering on your own key.** `agentsfleet` records your tokens but charges run time only; your provider's dashboard shows the token bill.
- **Ollama has no curated tags.** Its tags and prices come from the configured server, and no published token price exists.

## Related pages

- [Billing](./billing.md): what each posture costs
- [User flow](./user_flow.md): creating a fleet
- [How the runner runs a fleet](./runner_execution.md): the supervisor's provider client
- [Context lifecycle](./context_lifecycle.md): compaction near the model's window
- [`../AUTH.md`](../AUTH.md): scopes and tokens

# write_http.md — HTTP/REST surface dispatch (LATENT façade, route-and-delegate)

This is the prose the AGENT reads **before adding, modifying, or removing any HTTP
endpoint**. Unlike `write_zig`, it does **not** merge its rule doc — the canonical
761-line checklist `docs/REST_API_DESIGN_GUIDELINES.md` is a self-contained
product-surface design guide, so this façade *routes* to it rather than
duplicating it. The deterministic checks run in the **product repo** (`make lint`
/ `/review` against the REST guide), not in dotfiles — hence **⚪ delegated**.

**Signal legend:**

- ⚪ delegated — the REST checklist is enforced in the product repo (agentsfleet),
  by `make lint` + adversarial `/review` against the guide. Dotfiles
  carries only the routing + the discipline that the guide is a *checklist, not
  background reading*.

## Trigger — read `docs/REST_API_DESIGN_GUIDELINES.md` before

- Editing `src/http/handlers/**`, `public/openapi/**`, or any `route_*` file.
- Adding, modifying, or removing an HTTP endpoint or its OpenAPI shape.

**Override:** none from dotfiles — the REST guide's own `MUST`/`SHOULD`
semantics govern (a `MUST` violation blocks merge; a `SHOULD` deviation needs a
one-line PR rationale).

## What it routes to

[`docs/REST_API_DESIGN_GUIDELINES.md`](../docs/REST_API_DESIGN_GUIDELINES.md) —
canonical instruction set covering URL design (plural-noun resources, no verbs,
allowed `:verb` categories), path params + trailing-slash rules, HTTP-method
semantics + PATCH idempotency, the `202 + /v1/operations/{id}` long-running shape,
request/response body shapes, error envelopes, pagination, and the pre-PR §10
gate. Treat it as a checklist run at `CHORE(close)` before `gh pr create`.

## Why route, not merge

REST design rules bind the *product repo's* HTTP surface, which does not exist in
dotfiles. Merging 761 lines here would duplicate a doc with no dotfiles consumer
and create a second source of truth. The guide stays canonical; this façade is the
dispatch entry that points the agent at it when the trigger fires.

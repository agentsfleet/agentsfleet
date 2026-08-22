# write_http.md — HTTP/REST surface dispatch (LATENT façade, route-and-delegate)

This is the prose the AGENT reads **before adding, modifying, or removing any HTTP
endpoint**. Unlike `write_zig`, it does **not** merge its rule doc — the canonical
checklist `docs/REST_API_DESIGN_GUIDELINES.md` is a
self-contained product-surface design guide, so this façade *routes* to it rather
than duplicating it. **Residence vs enforcement:** `orly init` materialises the
guide into this repository under the `domain.http` pack — it is a real local
file, not something read through another checkout. Its checks are *enforced*
in the product repo (`make lint` / `/review` against the guide) — hence
**🟣 delegated**.

**Signal legend:**

- 🟣 delegated — the REST checklist is enforced in the product repo (agentsfleet),
  by `make lint` + adversarial `/review` against the guide. Dotfiles carries the
  doc itself, the routing, and the discipline that the guide is a *checklist, not
  background reading*.

## Trigger — read `docs/REST_API_DESIGN_GUIDELINES.md` before

- Editing `src/http/handlers/**`, `public/openapi/**`, or any `route_*` file.
- Adding, modifying, or removing an HTTP endpoint or its OpenAPI shape.

**Override:** none from dotfiles — the REST guide's own `MUST`/`SHOULD`
semantics govern (a `MUST` violation blocks merge; a `SHOULD` deviation needs a
one-line PR rationale).

## What it routes to

`docs/REST_API_DESIGN_GUIDELINES.md` — canonical instruction
set covering URL design (plural-noun resources, no verbs, allowed `:verb`
categories), path params + trailing-slash rules, HTTP-method semantics + PATCH
idempotency, the `202 + /v1/operations/{id}` long-running shape, request/response
body shapes, error envelopes, pagination, and the pre-PR §10 gate. Treat it as a
checklist run at `CHORE(close)` before `gh pr create`.

## Why route, not merge

REST design rules bind the *product repo's* HTTP surface, but the doc has one
canonical source in dotfiles — `orly init`/`orly update` materialise it here,
so this repository's copy tracks that source. Merging its full text into
this façade would create a second source of truth. The guide stays canonical;
this façade is the dispatch entry that points the agent at it when the trigger
fires.

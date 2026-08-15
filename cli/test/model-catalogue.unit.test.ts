// Unit tests for lib/model-catalogue.ts — the CLI's only source of provider
// and model truth now that constants/providers.ts is gone.
//
// The retired design was a 116-entry copy of NullClaw's dial tables plus a
// parity test that watched the copy drift. These tests assert the properties
// that replaced it: the accepted set comes from the wire, paging is followed to
// the end, the catalogue owns case-folding, and both degradations (unreachable,
// empty) fail OPEN so the server stays the arbiter.

import { test, expect } from "bun:test";
import { Effect, Layer } from "effect";

import { HttpClient } from "../src/services/http-client.ts";
import {
  catalogueProviders,
  fetchCatalogue,
  resolveCatalogueTarget,
  type LibraryModel,
} from "../src/lib/model-catalogue.ts";
import { OPENAI_COMPATIBLE_PROVIDER } from "../src/constants/custom-endpoint.ts";
import { NetworkError } from "../src/errors/index.ts";

const model = (id: string, provider: string): LibraryModel => ({ id, provider });

const ANTHROPIC = model("claude-opus-5", "anthropic");
const OPENAI = model("gpt-5.6-sol", "openai");

/** A mock catalogue endpoint. `pages` are returned in order, chained by cursor. */
const httpLayer = (
  pages: ReadonlyArray<{ models: ReadonlyArray<LibraryModel>; next_cursor?: string | null }>,
  onPath?: (path: string) => void,
): Layer.Layer<HttpClient> => {
  let call = 0;
  return Layer.succeed(HttpClient, {
    request: ((input: { path: string }) => {
      onPath?.(input.path);
      const page = pages[Math.min(call, pages.length - 1)];
      call += 1;
      return Effect.succeed(page);
    }) as HttpClient["request"],
  });
};

const failingLayer = (): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (() =>
      Effect.fail(
        new NetworkError({ detail: "catalogue unreachable", suggestion: "retry", url: "https://api.test.local/v1/models" }),
      )) as HttpClient["request"],
  });

const run = <A>(
  effect: Effect.Effect<A, unknown, HttpClient>,
  layer: Layer.Layer<HttpClient>,
): Promise<A> => Effect.runPromise(effect.pipe(Effect.provide(layer)) as Effect.Effect<A>);

test("catalogueProviders dedupes and sorts, ignoring blank provider fields", () => {
  const rows = [OPENAI, ANTHROPIC, model("claude-haiku-4-5", "anthropic"), model("x", "  ")];
  expect(catalogueProviders(rows)).toEqual(["anthropic", "openai"]);
});

test("fetchCatalogue asks for the server's maximum page size", async () => {
  const paths: string[] = [];
  await run(fetchCatalogue(undefined), httpLayer([{ models: [ANTHROPIC] }], (p) => paths.push(p)));
  // 100 is the endpoint's cap (UZ-LIBRARY-003 above it), so one round-trip
  // covers today's catalogue instead of silently taking a default-sized slice.
  expect(paths[0]).toContain("limit=100");
});

test("fetchCatalogue passes the provider filter through as a query parameter", async () => {
  const paths: string[] = [];
  await run(
    fetchCatalogue(undefined, { provider: "anthropic" }),
    httpLayer([{ models: [ANTHROPIC] }], (p) => paths.push(p)),
  );
  expect(paths[0]).toContain("provider=anthropic");
});

test("fetchCatalogue follows next_cursor to the end and concatenates every page", async () => {
  const paths: string[] = [];
  const rows = await run(
    fetchCatalogue(undefined),
    httpLayer(
      [
        { models: [ANTHROPIC], next_cursor: "cur-1" },
        { models: [OPENAI], next_cursor: null },
      ],
      (p) => paths.push(p),
    ),
  );
  // A single unpaged read would truncate the moment the catalogue outgrows one
  // page — exactly what seeding 103 providers makes likely.
  expect(rows).toHaveLength(2);
  expect(paths[1]).toContain("starting_after=cur-1");
});

test("fetchCatalogue stops at an empty-string cursor rather than looping", async () => {
  const rows = await run(
    fetchCatalogue(undefined),
    httpLayer([{ models: [ANTHROPIC], next_cursor: "" }]),
  );
  expect(rows).toHaveLength(1);
});

test("fetchCatalogue stops at the page cap when the server never stops paging", async () => {
  // A server that always hands back a cursor must not spin forever. The cap is
  // 100 pages — 10,000 rows, two orders of magnitude past any real catalogue —
  // so reaching it means the server is looping, not that the operator is large.
  const paths: string[] = [];
  const rows = await run(
    fetchCatalogue(undefined),
    httpLayer([{ models: [ANTHROPIC], next_cursor: "never-ends" }], (p) => paths.push(p)),
  );
  expect(paths).toHaveLength(100);
  expect(rows).toHaveLength(100);
});

test("resolveCatalogueProvider accepts a member and returns it unchanged", async () => {
  const got = await run(
    resolveCatalogueTarget("anthropic", undefined, undefined),
    httpLayer([{ models: [ANTHROPIC, OPENAI] }]),
  );
  expect(got.provider).toBe("anthropic");
});

test("resolveCatalogueProvider folds case to the CATALOGUE's spelling", async () => {
  const got = await run(
    resolveCatalogueTarget("AnThRoPiC", undefined, undefined),
    httpLayer([{ models: [ANTHROPIC] }]),
  );
  // Not the caller's bytes: the resolver compares the stored provider
  // byte-for-byte, so anything else stores a credential that cannot dial.
  expect(got.provider).toBe("anthropic");
});

test("resolveCatalogueProvider accepts the custom-endpoint sentinel without a catalogue row", async () => {
  const got = await run(
    resolveCatalogueTarget(OPENAI_COMPATIBLE_PROVIDER, undefined, undefined),
    httpLayer([{ models: [ANTHROPIC] }]),
  );
  expect(got.provider).toBe(OPENAI_COMPATIBLE_PROVIDER);
});

test("resolveCatalogueProvider rejects a non-member, naming what THIS server serves", async () => {
  const err = await Effect.runPromise(
    resolveCatalogueTarget("cerebras", undefined, undefined).pipe(
      Effect.provide(httpLayer([{ models: [ANTHROPIC] }])),
      Effect.flip,
    ) as unknown as Effect.Effect<{ detail: string; suggestion?: string }>,
  );
  expect(err.detail).toContain("cerebras");
  expect(err.detail).toContain("anthropic");
  // The sentinel is in the accepted set without being a catalogue row — it
  // names a user-supplied endpoint, so no seeded model could ever imply it.
  expect(err.detail).toContain(OPENAI_COMPATIBLE_PROVIDER);
  expect(err.suggestion).toContain("agentsfleet models");
});

test("a CLI-engine name is refused with its reason instead of the accepted-set wall", async () => {
  const err = await Effect.runPromise(
    resolveCatalogueTarget("claude-cli", undefined, undefined).pipe(
      Effect.provide(httpLayer([{ models: [ANTHROPIC] }])),
      Effect.flip,
    ) as unknown as Effect.Effect<{ detail: string }>,
  );
  expect(err.detail).toContain("carries no API key");
  // Printing both would bury the sentence that explains the refusal.
  expect(err.detail).not.toContain("is not in this server's model catalogue");
});

test("a model the provider serves is accepted and returned unchanged", async () => {
  const got = await run(
    resolveCatalogueTarget("anthropic", "claude-opus-5", undefined),
    httpLayer([{ models: [ANTHROPIC, OPENAI] }]),
  );
  expect(got).toEqual({ provider: "anthropic", model: "claude-opus-5" });
});

test("a model is NOT case-folded — model ids belong to the provider", async () => {
  // `MiniMaxAI/MiniMax-M3` and `accounts/fireworks/models/kimi-k3` are real
  // catalogue ids. Folding one would invent an id no provider serves, so a
  // wrong-case model is a rejection, never a silent correction.
  const err = await Effect.runPromise(
    resolveCatalogueTarget("anthropic", "CLAUDE-OPUS-5", undefined).pipe(
      Effect.provide(httpLayer([{ models: [ANTHROPIC] }])),
      Effect.flip,
    ) as unknown as Effect.Effect<{ detail: string }>,
  );
  expect(err.detail).toContain("CLAUDE-OPUS-5");
});

test("a model the provider does not serve is rejected, listing that provider's models", async () => {
  const err = await Effect.runPromise(
    resolveCatalogueTarget("anthropic", "gpt-5.6-sol", undefined).pipe(
      Effect.provide(httpLayer([{ models: [ANTHROPIC, OPENAI] }])),
      Effect.flip,
    ) as unknown as Effect.Effect<{ detail: string; suggestion?: string }>,
  );
  expect(err.detail).toContain("gpt-5.6-sol");
  expect(err.detail).toContain("claude-opus-5");
  // Scoped to the provider: openai's models are not offered as alternatives
  // for an anthropic credential.
  expect(err.detail).toContain("provider 'anthropic'");
  expect(err.suggestion).toContain("models --provider anthropic");
});

test("no --model skips the model check entirely", async () => {
  // `--model` is required by the body composer, not by this resolver, and a
  // `--data` blob reaches neither. Absent means "nothing to check".
  const got = await run(
    resolveCatalogueTarget("anthropic", undefined, undefined),
    httpLayer([{ models: [ANTHROPIC] }]),
  );
  expect(got.model).toBeUndefined();
});

test("the sentinel accepts any model without a catalogue read", async () => {
  // A user-supplied endpoint serves whatever it serves; no catalogue row could
  // describe it, so reading the catalogue to approve it is a question whose
  // answer is already known.
  let reads = 0;
  const got = await run(
    resolveCatalogueTarget(OPENAI_COMPATIBLE_PROVIDER, "some-local-build", undefined),
    httpLayer([{ models: [ANTHROPIC] }], () => { reads += 1; }),
  );
  expect(got).toEqual({ provider: OPENAI_COMPATIBLE_PROVIDER, model: "some-local-build" });
  expect(reads).toBe(0);
});

test("an UNREACHABLE catalogue accepts the provider — the server stays the arbiter", async () => {
  // The dashboard degrades to a free-text provider input on the same condition.
  // Refusing here would make a catalogue outage mean "you may not store a
  // credential", a worse failure than one the server rejects with a typed error.
  const got = await run(resolveCatalogueTarget("whatever", undefined, undefined), failingLayer());
  expect(got.provider).toBe("whatever");
});

test("an EMPTY catalogue accepts the provider — a fresh environment stays usable", async () => {
  // core.model_library ships empty; the model_catalogue playbook fills it.
  const got = await run(
    resolveCatalogueTarget("anthropic", undefined, undefined),
    httpLayer([{ models: [] }]),
  );
  expect(got.provider).toBe("anthropic");
});

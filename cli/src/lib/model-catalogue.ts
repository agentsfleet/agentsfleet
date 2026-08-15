// The model catalogue, read from `GET /v1/models`.
//
// This is the CLI's ONLY answer to "which providers and models exist". It used
// to carry `constants/providers.ts`, a 116-entry hand-copy of NullClaw's
// `classifyProvider` tables, and the dashboard answered the same question from
// `core.model_library` — two surfaces, two lists, and a parity test whose whole
// job was to notice they had diverged. A test that watches a copy drift is not
// a fix for the copy.
//
// So the copy is gone. `AddModelEntryDialog.tsx` derives its provider dropdown
// from `uniqueProviders(models)` over this same endpoint; the CLI now derives
// the accepted `--provider` set the same way, from the same bytes. A provider
// reaches both surfaces by being seeded into the catalogue
// (scripts/model-library-allowlist.json → the model_catalogue playbook), never
// by editing TypeScript.
//
// The catalogue is bearer-authed (`handlers/model_library.zig`: "the catalogue
// prices the platform's billing spine and has no anonymous consumer"), which is
// why every read here takes a token.

import { Effect } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { MODEL_LIBRARY_PATH, QUERY_LIMIT, QUERY_PROVIDER, QUERY_STARTING_AFTER } from "./api-paths.ts";
import { OPENAI_COMPATIBLE_PROVIDER } from "../constants/custom-endpoint.ts";
import { ValidationError, type CliError } from "../errors/index.ts";
import type { Redacted } from "effect/Redacted";

/** One priced catalogue row. Mirrors the wire shape in handlers/model_library_page.zig. */
export interface LibraryModel {
  readonly id?: string;
  readonly provider?: string;
  readonly context_cap_tokens?: number;
  readonly input_nanos_per_mtok?: number;
  readonly cached_input_nanos_per_mtok?: number;
  readonly output_nanos_per_mtok?: number;
}

interface ModelLibraryPage {
  readonly version?: string;
  readonly models?: ReadonlyArray<LibraryModel>;
  readonly next_cursor?: string | null;
}

// The endpoint caps `limit` at 100 and rejects anything larger with
// UZ-LIBRARY-003. Ask for the maximum so a catalogue the size of the current
// allowlist arrives in one round-trip, and follow `next_cursor` for the rest —
// a single unpaged read would silently truncate the moment the catalogue grows
// past one page, which is exactly what the 103-provider seeding makes likely.
const PAGE_LIMIT = 100;

// A runaway `next_cursor` must not spin forever. 100 pages is 10,000 rows —
// two orders of magnitude beyond any real catalogue, so hitting it means the
// server is looping, not that the operator has a big catalogue.
const MAX_PAGES = 100;

interface CatalogueReadOptions {
  /** Restrict the read to one provider's rows (server-side `?provider=`). */
  readonly provider?: string | undefined;
}

/**
 * Every catalogue row, following `next_cursor` to the end.
 *
 * Errors propagate: a caller that can degrade (the `--provider` check) catches
 * them, and a caller that cannot (the `models` command) reports them.
 */
export const fetchCatalogue = (
  token: Redacted<string> | undefined,
  options: CatalogueReadOptions = {},
): Effect.Effect<ReadonlyArray<LibraryModel>, CliError, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const collected: LibraryModel[] = [];
    let cursor: string | undefined;

    for (let page = 0; page < MAX_PAGES; page += 1) {
      const params = new URLSearchParams({ [QUERY_LIMIT]: String(PAGE_LIMIT) });
      if (options.provider) params.set(QUERY_PROVIDER, options.provider);
      if (cursor) params.set(QUERY_STARTING_AFTER, cursor);

      const res = yield* http.request<ModelLibraryPage>({
        path: `${MODEL_LIBRARY_PATH}?${params.toString()}`,
        token,
      });

      collected.push(...(res.models ?? []));
      const next = res.next_cursor;
      if (typeof next !== "string" || next.length === 0) return collected;
      cursor = next;
    }
    return collected;
  });

/** Provider ids the catalogue actually serves, sorted, deduped. */
export const catalogueProviders = (
  models: ReadonlyArray<LibraryModel>,
): ReadonlyArray<string> => {
  const seen = new Set<string>();
  for (const m of models) {
    const p = m.provider?.trim();
    if (p) seen.add(p);
  }
  return [...seen].sort();
};

/**
 * The full accepted `--provider` set: every catalogue provider plus the
 * custom-endpoint sentinel, which is deliberately NOT a catalogue row — it
 * names a user-supplied endpoint, so no seeded model could ever imply it.
 * The dashboard pins it last in its dropdown for the same reason.
 *
 * Module-private: the only consumer is the rejection path below, and the set it
 * builds is observable in that rejection's message. Exporting it would add a
 * public surface whose sole caller lives four lines away.
 */
const acceptedProviders = (
  models: ReadonlyArray<LibraryModel>,
): ReadonlyArray<string> => [...catalogueProviders(models), OPENAI_COMPATIBLE_PROVIDER];

/**
 * Names NullClaw dials by spawning a local coding-agent binary. They carry no
 * API key, so they are deliberately absent from the catalogue and always will
 * be — `gen-provider-skeleton.mjs` drops them when it derives the allowlist.
 *
 * This is NOT a provider list and never widens or narrows what is accepted: it
 * only replaces "not in this server's catalogue" with the actual reason for the
 * one mistake a catalogue rejection explains badly. Someone typing `claude-cli`
 * has a specific wrong model in their head, and the accepted-set wall does not
 * correct it.
 */
const CLI_ENGINE_REJECTION =
  "spawns a local CLI and carries no API key, so it cannot back a stored credential yet";

const CLI_ENGINE_NAMES: ReadonlyArray<string> = [
  "claude-cli",
  "claude-code",
  "codex-cli",
  "gemini-cli",
  "openai-codex",
];

const isCliEngine = (provider: string): boolean =>
  CLI_ENGINE_NAMES.includes(provider.toLowerCase());

// Both rejection messages list a set, and both joined it with the same
// separator literal (RULE UFS).
const LIST_SEPARATOR = ", ";

// Absent from the catalogue means UNPRICED, not unreachable. Most of these
// names are dialable — NullClaw's compat table carries ~100 endpoints the
// catalogue does not price yet — and every one of them is still usable through
// the custom-endpoint sentinel. A bare "not in this server's catalogue" reads
// as "you cannot use this provider", which is false and sends the operator
// looking for a permission they do not need.
const PROVIDER_REJECTED = (provider: string, accepted: ReadonlyArray<string>): string => {
  // The reason replaces the wall rather than joining it — printing both buries
  // the sentence that actually explains the refusal.
  if (isCliEngine(provider)) return `provider '${provider}' ${CLI_ENGINE_REJECTION}`;
  return (
    `provider '${provider}' is not priced in this server's model catalogue. ` +
    `Priced: ${accepted.join(LIST_SEPARATOR)}`
  );
};

const CUSTOM_ENDPOINT_ROUTE =
  `--provider ${OPENAI_COMPATIBLE_PROVIDER} --base-url https://host/v1 --model <m>`;

// The sentinel route is the answer for an unpriced provider and NOT the answer
// for a CLI engine, which spawns a binary and has no HTTP endpoint to point at.
// Offering it there would send the operator to invent a URL that cannot exist.
const PROVIDER_SUGGESTION = (provider: string): string =>
  isCliEngine(provider)
    ? "use a provider that authenticates with an API key; run `agentsfleet models` to see them"
    : `run \`agentsfleet models\` to see what this server prices, or reach it directly with ${CUSTOM_ENDPOINT_ROUTE}`;

export interface CatalogueTarget {
  /** The catalogue's spelling of the provider — what gets stored. */
  readonly provider: string;
  /** The model, unchanged. Model ids are case-sensitive at every provider. */
  readonly model: string | undefined;
}

/** Models one provider serves, by catalogue id, sorted. */
const modelsForProvider = (
  models: ReadonlyArray<LibraryModel>,
  provider: string,
): ReadonlyArray<string> => {
  const ids = new Set<string>();
  for (const m of models) {
    if (m.provider === provider && m.id) ids.add(m.id);
  }
  return [...ids].sort();
};

const MODEL_REJECTED = (
  model: string,
  provider: string,
  known: ReadonlyArray<string>,
): string =>
  `model '${model}' is not in this server's catalogue for provider '${provider}'. ` +
  `Available: ${known.join(LIST_SEPARATOR)}`;

const MODEL_SUGGESTION = (provider: string): string =>
  `run \`agentsfleet models --provider ${provider}\` for the list`;

/**
 * Resolve BOTH identifiers against one catalogue read.
 *
 * `--model` was validated nowhere: a typo stored a credential that reported
 * success and failed at the first event, which is the same defect `--provider`
 * had one flag over. The catalogue already knows every model id per provider,
 * so closing it costs one lookup on a page this call already fetched — not a
 * second round-trip.
 *
 * The model is NOT case-folded. Provider ids are ours to canonicalise; model
 * ids belong to the provider and are case-sensitive there
 * (`accounts/fireworks/models/kimi-k3`, `MiniMaxAI/MiniMax-M3`), so folding
 * one would invent an id the provider does not serve.
 *
 * Three deliberate degradations, all matching what the dashboard does rather
 * than inventing a stricter CLI:
 *
 *   - the read FAILS (offline, expired token, server down) — accept and let
 *     the server arbitrate. The dashboard falls back to a free-text provider
 *     input on the same condition (`AddModelEntryDialog.tsx`,
 *     `providerOptions.length > 0 ? Select : Input`). Refusing would make an
 *     unreachable catalogue mean "you may not store a credential" — a worse
 *     failure than one the server rejects with a typed error.
 *   - the catalogue is EMPTY — a freshly deployed environment before the
 *     model_catalogue playbook runs. Rejecting everything there would make the
 *     CLI unusable during exactly the provisioning it is used for.
 *   - the provider is the custom-endpoint sentinel — a user-supplied endpoint
 *     serves whatever it serves, and no catalogue row could describe it.
 *
 * Every path still hits full server-side validation, so this buys a fast,
 * local, accurate error message — never a security boundary.
 */
export const resolveCatalogueTarget = (
  provider: string,
  model: string | undefined,
  token: Redacted<string> | undefined,
): Effect.Effect<CatalogueTarget, CliError, HttpClient> =>
  Effect.gen(function* () {
    if (provider === OPENAI_COMPATIBLE_PROVIDER) return { provider, model };

    const models = yield* fetchCatalogue(token).pipe(
      Effect.orElseSucceed((): ReadonlyArray<LibraryModel> => []),
    );
    if (models.length === 0) return { provider, model };

    const accepted = acceptedProviders(models);
    const match = accepted.find((id) => id.toLowerCase() === provider.toLowerCase());
    if (!match) {
      return yield* Effect.fail(
        new ValidationError({
          detail: PROVIDER_REJECTED(provider, accepted),
          suggestion: PROVIDER_SUGGESTION(provider),
        }),
      );
    }

    const trimmed = model?.trim();
    if (!trimmed) return { provider: match, model };

    // A provider only reaches `accepted` by owning at least one row, so this
    // list is never empty and the check is always meaningful.
    const known = modelsForProvider(models, match);
    if (!known.includes(trimmed)) {
      return yield* Effect.fail(
        new ValidationError({
          detail: MODEL_REJECTED(trimmed, match, known),
          suggestion: MODEL_SUGGESTION(match),
        }),
      );
    }
    return { provider: match, model: trimmed };
  });

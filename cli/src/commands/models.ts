// `agentsfleet models` — the priced model catalogue this server serves.
//
// The CLI peer of the dashboard's model picker. Both read `GET /v1/models`;
// this is the terminal rendering of the same rows the Add Model dialog puts in
// a dropdown. Before it existed the CLI had no way to ask what a provider or
// model id should be: `--provider` was checked against a vendored copy of
// NullClaw's dial table and `--model` was checked against nothing at all, so
// the flow was "type two identifiers blind, discover at run time".
//
// Rates print as United States Dollars per million tokens, converted from the
// nanos the wire carries. They are charged only under platform-managed
// posture; a self-managed credential pays the run fee and is billed by the
// operator's own provider account, so a zero rate is a real catalogue state
// (schema/400_model_library.sql) and prints as a dash rather than $0.00.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { catalogueProviders, fetchCatalogue, type LibraryModel } from "../lib/model-catalogue.ts";
import { OPENAI_COMPATIBLE_PROVIDER } from "../constants/custom-endpoint.ts";
import { ui } from "../output/index.ts";
import type { CliError } from "../errors/index.ts";

const FIELD_PROVIDER = "provider" as const;
const FIELD_MODEL = "model" as const;
const FIELD_CONTEXT = "context" as const;
const FIELD_INPUT = "input" as const;
const FIELD_OUTPUT = "output" as const;

const NANOS_PER_USD = 1_000_000_000;
const UNPRICED = "—" as const;
const TOKENS_PER_K = 1_000;

export interface ModelsFlags {
  readonly provider?: string | undefined;
}

// Two decimals is right for a dollar, and wrong for a rate below a cent —
// $0.003625 per Mtok would print as "$0.00", which is this table's signal for
// "no rate at all". A priced model must never be indistinguishable from an
// unpriced one, so a sub-cent rate switches to significant digits instead.
const SUB_CENT = 0.01;
const SUB_CENT_DIGITS = 2;

/** Nanos per million tokens → "$1.25", or a dash when the row carries no rate. */
const usd = (nanos: number | undefined): string => {
  if (!nanos || nanos <= 0) return UNPRICED;
  const dollars = nanos / NANOS_PER_USD;
  return dollars < SUB_CENT
    ? `$${dollars.toPrecision(SUB_CENT_DIGITS)}`
    : `$${dollars.toFixed(SUB_CENT_DIGITS)}`;
};

/** 200000 → "200k". The exact number is in --json; the table wants a shape. */
const contextLabel = (tokens: number | undefined): string =>
  !tokens || tokens <= 0 ? UNPRICED : `${Math.round(tokens / TOKENS_PER_K)}k`;

const row = (m: LibraryModel): Record<string, string> => ({
  provider: String(m.provider ?? ""),
  model: String(m.id ?? ""),
  context: contextLabel(m.context_cap_tokens),
  input: usd(m.input_nanos_per_mtok),
  output: usd(m.output_nanos_per_mtok),
});

export const modelsEffectFromFlags = (
  flags: ModelsFlags,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;

    const token = yield* resolveAuthToken;
    const provider = flags.provider?.trim();
    const models = yield* fetchCatalogue(token, { provider });

    if (config.jsonMode) {
      yield* output.printJson({ models });
      return;
    }

    if (models.length === 0) {
      // An empty catalogue is a provisioning state, not an error: the table
      // ships empty and the model_catalogue playbook fills it. Say which,
      // because "no models" with no cause reads as a broken server.
      yield* output.info(
        provider
          ? `No models for provider '${provider}'. Run \`agentsfleet models\` for the full catalogue.`
          : "This server's model catalogue is empty — a platform admin primes it from scripts/model-library-allowlist.json.",
      );
      return;
    }

    yield* output.printTable(
      [
        { key: FIELD_PROVIDER, label: "PROVIDER" },
        { key: FIELD_MODEL, label: "MODEL" },
        { key: FIELD_CONTEXT, label: "CONTEXT" },
        { key: FIELD_INPUT, label: "IN/MTOK" },
        { key: FIELD_OUTPUT, label: "OUT/MTOK" },
      ],
      models.map(row),
    );

    const providers = catalogueProviders(models);
    yield* output.info(
      ui.dim(
        `${models.length} model(s) across ${providers.length} provider(s). ` +
          `Store a credential with: agentsfleet secret create <name> --provider <id> --api-key <key> --model <m>`,
      ),
    );
    if (!provider) {
      yield* output.info(
        ui.dim(
          `For an endpoint this catalogue does not carry, use --provider ${OPENAI_COMPATIBLE_PROVIDER} --base-url https://host/v1`,
        ),
      );
    }
  });

// `agentsfleet models` — the priced model catalogue this server serves.
//
// The CLI peer of the dashboard's model picker. Both read `GET /v1/models`;
// this is the terminal rendering of the same rows the Add Model dialog puts in
// a dropdown. It answers what a provider or model id should be before either
// is typed into `--provider` or `--model`, so a wrong identifier shows up here
// rather than at run time.
//
// Rates print as United States Dollars per million tokens, converted from the
// nanos the wire carries. They are charged only under platform-managed
// posture; a self-managed credential pays the run fee and is billed by the
// operator's own provider account. Zero rates remain explicit; a catalogue
// price never substitutes for the selected credential's billing posture.

import { Effect } from "effect";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { catalogueProviders, fetchCatalogue, type LibraryModel } from "../lib/model-catalogue.ts";
import { OPENAI_COMPATIBLE_PROVIDER } from "../constants/custom-endpoint.ts";
import { ui } from "../output/index.ts";
import { formatRate } from "../constants/billing.ts";

const FIELD_PROVIDER = "provider" as const;
const FIELD_MODEL = "model" as const;
const FIELD_NAME = "name" as const;
const FIELD_CONTEXT = "context" as const;
const FIELD_INPUT = "input" as const;
const FIELD_OUTPUT = "output" as const;
const FIELD_CACHED = "cached" as const;

const MODELS_LISTED = "Model catalogue" as const;

export interface ModelsFlags {
  readonly provider?: string | undefined;
  readonly advanced?: boolean | undefined;
}

const row = (m: LibraryModel): Record<string, string> => ({
  provider: m.provider, model: m.id, name: m.display_name,
  context: m.context_cap_tokens.toLocaleString("en-US"),
  input: formatRate(m.input_nanos_per_mtok),
  cached: formatRate(m.cached_input_nanos_per_mtok),
  output: formatRate(m.output_nanos_per_mtok),
});

export const modelsEffectFromFlags = Effect.fn("models.list")(function* (
  flags: ModelsFlags,
) {
  const output = yield* Output;

  const token = yield* resolveAuthToken;
  const provider = flags.provider?.trim();
  const models = yield* fetchCatalogue(token, { provider, advanced: flags.advanced });

  if (output.format !== OUTPUT_FORMAT.text) {
    yield* output.success(MODELS_LISTED, { models });
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

  yield* output.printEntityTable(
    {
      name: { key: FIELD_MODEL, label: "MODEL" },
      domain: [
        { key: FIELD_NAME, label: "NAME" },
        { key: FIELD_PROVIDER, label: "PROVIDER" },
        { key: FIELD_CONTEXT, label: "CONTEXT" },
        { key: FIELD_INPUT, label: "IN/MTOK" },
        { key: FIELD_CACHED, label: "CACHED/MTOK" },
        { key: FIELD_OUTPUT, label: "OUT/MTOK" },
      ],
      // A published catalogue is not something this workspace made, so there
      // is no age to report.
      ageKey: null,
    },
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

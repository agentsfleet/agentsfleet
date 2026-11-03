import { afterAll, afterEach, beforeEach, describe, expect, test } from "bun:test";
import type { SQL } from "bun";
import { CATALOGUE_DATABASE_URL, openIntegrationDatabase, removeFixtureModels } from "./integration-fixtures";
import { seedCatalogue } from "./seed";
import { catalogueFixture, MODEL_NAME, modelFixture, priceFixture, VERIFIED_AT } from "./test-fixtures";

const database = openIntegrationDatabase();
const PRUNED = "pruned-coding-with-decisions";
const DECISION_ONLY = "decision-only-fixture";
const REVISION = "reviewed-fixture-1";

describe.skipIf(!CATALOGUE_DATABASE_URL)("coding imports preserve decision ownership", () => {
  beforeEach(() => removeFixtureModels(database));
  afterEach(() => removeFixtureModels(database));
  afterAll(() => database.close());

  test("apply, pruning and repeated empty imports retain independent decision definitions and quotes", async () => {
    await seedCatalogue(database, catalogueFixture([
      modelFixture({ prices: [priceFixture(), priceFixture({ prompt_tokens_above: 100_000 })] }),
      modelFixture({ model_name: PRUNED }), modelFixture({ model_name: DECISION_ONLY }),
    ]), true);
    for (const model of [MODEL_NAME, PRUNED, DECISION_ONLY]) await attachDecision(database, model);
    await database`DELETE FROM model.platform_model_prices
      WHERE platform_model_id IN (SELECT id FROM model.platform_models WHERE model_name = ${DECISION_ONLY})`;
    await database`UPDATE model.platform_models SET inference = NULL WHERE model_name = ${DECISION_ONLY}`;
    const before = await decisionRows(database);
    const wanted = catalogueFixture([modelFixture({ prices: [priceFixture({ input: "0.25" })] })]);
    expect((await seedCatalogue(database, wanted, true)).diff).toEqual({
      added: [], changed: [`fireworks/${MODEL_NAME}`], deleted: [`fireworks/${PRUNED}`],
    });
    expect(await decisionRows(database)).toEqual(before);
    expect(await database`SELECT model_name FROM model.platform_models ORDER BY model_name`)
      .toEqual([MODEL_NAME, DECISION_ONLY, PRUNED].toSorted().map((model_name) => ({ model_name })));
    expect(await database`SELECT inference FROM model.platform_models WHERE model_name = ${PRUNED}`).toEqual([{ inference: null }]);
    expect(await database`SELECT input_nanos_per_mtok::text AS rate FROM model.platform_model_prices`)
      .toEqual([{ rate: "250000000" }]);
    expect((await seedCatalogue(database, wanted, true)).applied).toBe(false);
    expect((await seedCatalogue(database, catalogueFixture([]), true)).diff)
      .toEqual({ added: [], changed: [], deleted: [`fireworks/${MODEL_NAME}`] });
    expect(await decisionRows(database)).toEqual(before);
    expect(await database`SELECT count(*)::integer AS configured FROM model.platform_models WHERE inference IS NOT NULL`)
      .toEqual([{ configured: 0 }]);
    expect(await seedCatalogue(database, catalogueFixture([]), true))
      .toEqual({ diff: { added: [], changed: [], deleted: [] }, applied: false });
  });
});

async function decisionRows(client: SQL): Promise<unknown> {
  return client`SELECT r.platform_model_id::text, r.revision, r.definition, r.created_at,
    m.decision_revision_id::text, m.decision_enabled,
    (SELECT jsonb_agg(p ORDER BY id) FROM model.decision_prices p
      WHERE p.platform_model_id = m.id) AS prices
    FROM model.decision_revisions r JOIN model.platform_models m ON m.id = r.platform_model_id
    ORDER BY r.platform_model_id, r.revision`;
}

async function attachDecision(client: SQL, model: string): Promise<void> {
  await client.begin(async (transaction) => {
    const revisionId = Bun.randomUUIDv7();
    await transaction.unsafe(`INSERT INTO model.decision_revisions (id, platform_model_id, revision, definition, created_at)
      SELECT $4::uuid, id, $2, $3::jsonb, now() FROM model.platform_models WHERE model_name = $1`,
    [model, REVISION, definition(), revisionId]);
    await transaction`UPDATE model.platform_models SET decision_revision_id = ${revisionId}::uuid, decision_enabled = ${model === MODEL_NAME}
      WHERE model_name = ${model}`;
    await transaction`INSERT INTO model.decision_prices
      (id, platform_model_id, decision_revision_id, effective_from_at, prompt_tokens_above,
       input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
       source_url, verified_at, conditions, created_at, updated_at)
      SELECT ${Bun.randomUUIDv7()}::uuid, m.id, ${revisionId}::uuid, p.effective_from, 0, 900, 90, 4500,
        'https://example.test/decision-pricing', p.verified_at, 'Synthetic decision quote.', now(), now()
      FROM model.platform_models m JOIN model.platform_model_prices p ON p.platform_model_id = m.id
      WHERE m.model_name = ${model} AND p.prompt_tokens_above = 0`;
  });
}

function definition() {
  return {
    revision: REVISION, api: "typesafe", endpoint: "https://api.typesafe.ai/v1/systemone",
    accepted_models: ["evaluated-fixture-model"], capabilities: { noul: true, choice: true, score: true }, input_max_bytes: 24576,
    intent: {
      questions_revision: "dt-routing-v1", intent: { probability_bps: 8500, confidence_bps: 7000 },
      association: { probability_bps: 9500, confidence_bps: 8500 }, single_request_max_bps: 3000, multiple_request_min_bps: 7000,
    },
    goal: {
      questions_revision: "dt-goal-v1", statement_min_bps: 9500,
      continue_action: { probability_bps: 8000, confidence_bps: 7000 }, wait: { probability_bps: 8500, confidence_bps: 7500 },
      done: { probability_bps: 9500, confidence_bps: 8500 },
    },
    evaluation_digest: "ab".repeat(32), evaluation_source_url: "https://example.test/decision-evaluation", evaluated_at: VERIFIED_AT,
  };
}

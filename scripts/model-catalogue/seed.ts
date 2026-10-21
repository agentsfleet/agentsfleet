import type { Catalogue } from "./catalogue";
import { SQL } from "bun";
import { z } from "zod";
import { catalogueDiff, desiredRows, hasChanges, modelRowsSchema, type CatalogueDiff, type ModelRow } from "./rows";
import { BUMP_VERSION, DELETE_MODELS, DELETE_PRICES, LOCK_VERSION, SELECT_ROWS, UPSERT_MODELS, UPSERT_PRICES } from "./statements";

type QueryClient = Pick<SQL, "unsafe">;
export type SeedResult = { diff: CatalogueDiff; applied: boolean };
const VERSION_ROWS = z.array(z.object({ resource_version: z.string().regex(/^\d+$/) })).length(1);
const TIMEOUT_SECONDS = 5;
const POOL_SIZE = 1;

export function openCatalogueDatabase(url: string): SQL {
  return new SQL(url, { max: POOL_SIZE, connectionTimeout: TIMEOUT_SECONDS, idleTimeout: TIMEOUT_SECONDS });
}

/** The singleton lock, catalogue writes and generation increment commit together. */
export async function seedCatalogue(database: SQL, catalogue: Catalogue, apply = false): Promise<SeedResult> {
  const wanted = desiredRows(catalogue);
  if (!apply) return { diff: catalogueDiff(wanted, await readRows(database)), applied: false };
  return database.begin(async (transaction) => {
    VERSION_ROWS.parse(await transaction.unsafe(LOCK_VERSION));
    const diff = catalogueDiff(wanted, await readRows(transaction));
    if (!hasChanges(diff)) return { diff, applied: false };
    await writeRows(transaction, wanted);
    return { diff, applied: true };
  });
}

export function seedReport(result: SeedResult): string[] {
  const { diff } = result;
  const lines = [
    ...diff.added.map((key) => `  + ${key}`),
    ...diff.changed.map((key) => `  ~ ${key}`),
    ...diff.deleted.map((key) => `  - ${key}`),
    `${diff.added.length} added; ${diff.changed.length} changed; ${diff.deleted.length} deleted`,
  ];
  return [...lines, result.applied ? "✓ catalogue and generation committed" : "✓ diff only; no rows written"];
}

async function readRows(database: QueryClient): Promise<ModelRow[]> {
  return modelRowsSchema.parse(await database.unsafe(SELECT_ROWS));
}

async function writeRows(database: QueryClient, wanted: ModelRow[]): Promise<void> {
  const identities = wanted.map(({ prices: _, ...identity }) => ({ id: Bun.randomUUIDv7(), ...identity }));
  const prices = wanted.flatMap((model) => model.prices.map((price) => ({
    id: Bun.randomUUIDv7(), provider: model.provider, model_name: model.model_name, ...price,
  })));
  await database.unsafe(UPSERT_MODELS, [identities]);
  await database.unsafe(UPSERT_PRICES, [prices]);
  await database.unsafe(DELETE_PRICES, [prices]);
  await database.unsafe(DELETE_MODELS, [identities]);
  await database.unsafe(BUMP_VERSION);
}

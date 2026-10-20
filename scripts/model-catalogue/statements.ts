/** Load the seeder's statements once; query-plan proofs execute these same values. */
const SQL_DIRECTORY = `${import.meta.dir}/sql`;

export const LOCK_VERSION = await Bun.file(`${SQL_DIRECTORY}/lock_version.sql`).text();
export const SELECT_ROWS = await Bun.file(`${SQL_DIRECTORY}/select_rows.sql`).text();
export const UPSERT_MODELS = await Bun.file(`${SQL_DIRECTORY}/upsert_models.sql`).text();
export const UPSERT_PRICES = await Bun.file(`${SQL_DIRECTORY}/upsert_prices.sql`).text();
export const DELETE_PRICES = await Bun.file(`${SQL_DIRECTORY}/delete_prices.sql`).text();
export const DELETE_MODELS = await Bun.file(`${SQL_DIRECTORY}/delete_models.sql`).text();
export const BUMP_VERSION = await Bun.file(`${SQL_DIRECTORY}/bump_version.sql`).text();

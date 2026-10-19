import { z } from "zod";
import { CATALOGUE_URL, catalogueReport, readCatalogue } from "./catalogue";
import { catalogueDiff, desiredRows } from "./rows";
import { assertCompleteCatalogue, CURATED_IDENTITIES } from "./curated";
import { openCatalogueDatabase, seedCatalogue, seedReport } from "./seed";

type CommandOptions = { args: readonly string[]; databaseUrl: string | undefined; cataloguePath?: string | URL };
type Output = (line: string) => void;
const APPLY_FLAG = "--apply";
const POSTGRES_PROTOCOLS = new Set(["postgres:", "postgresql:"]);
const argumentsSchema = z.array(z.literal(APPLY_FLAG)).max(1);
const connectionSchema = z.url().max(8192).refine((url) => POSTGRES_PROTOCOLS.has(new URL(url).protocol));
const errorCodeSchema = z.object({ code: z.string().regex(/^[A-Z0-9_]{1,64}$/) });
const CURATED_MODEL_NAMES = new Set(CURATED_IDENTITIES.map((identity) => identity.slice(identity.indexOf("/") + 1)));

/** Connection strings remain in process memory and never enter child arguments or errors. */
export async function runSeedCommand(options: CommandOptions, output: Output = console.log): Promise<number> {
  try {
    const apply = argumentsSchema.parse(options.args).includes(APPLY_FLAG);
    const catalogue = await readForCommand(options.cataloguePath ?? CATALOGUE_URL, output);
    if (options.cataloguePath === undefined) assertCompleteCatalogue(catalogue.models);
    catalogueReport(catalogue).forEach((line) => output(line));
    if (!options.databaseUrl) {
      if (apply) { output("✗ --apply requires DATABASE_URL"); return 1; }
      output("→ DATABASE_URL unset; showing the diff against an empty catalogue");
      seedReport({ diff: catalogueDiff(desiredRows(catalogue), []), applied: false }).forEach((line) => output(line));
      return 0;
    }
    const database = openCatalogueDatabase(connectionSchema.parse(options.databaseUrl));
    try {
      seedReport(await seedCatalogue(database, catalogue, apply)).forEach((line) => output(line));
    } finally {
      await database.close();
    }
    return 0;
  } catch (error) {
    const code = errorCodeSchema.safeParse(error);
    output(`✗ catalogue refresh failed (${code.success ? code.data.code : "validation or connection error"}); no credentials logged`);
    return 1;
  }
}

/** Only known public model identifiers may leave a catalogue validation failure. */
async function readForCommand(path: string | URL, output: Output) {
  try {
    return await readCatalogue(path);
  } catch (error) {
    if (error instanceof z.ZodError) {
      for (const name of CURATED_MODEL_NAMES) {
        if (error.issues.some((issue) => issue.message.startsWith(`${name}:`)))
          output(`✗ invalid catalogue entry ${name}; verify its context evidence, dates, sources and rates`);
      }
    }
    throw error;
  }
}

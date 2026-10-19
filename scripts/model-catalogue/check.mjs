import { CATALOGUE_URL, catalogueReport, readCatalogue } from "./catalogue.ts";
import { assertCompleteCatalogue } from "./curated.ts";

const catalogue = await readCatalogue(Bun.argv[2] ?? CATALOGUE_URL);
assertCompleteCatalogue(catalogue.models);
for (const line of catalogueReport(catalogue)) console.log(line);

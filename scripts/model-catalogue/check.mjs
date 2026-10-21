import { CATALOGUE_URL, catalogueReport, readCatalogue } from "./catalogue.ts";

const catalogue = await readCatalogue(Bun.argv[2] ?? CATALOGUE_URL);
for (const line of catalogueReport(catalogue)) console.log(line);

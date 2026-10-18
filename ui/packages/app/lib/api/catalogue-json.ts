import { nanosSchema } from "./catalogue-types";

// Native JSON.rawJSON is part of the runtime; the compiler library omits its type.
const sourceJSON = JSON as JSON & { rawJSON(value: string): unknown };

const NANOS_SUFFIX = "_nanos_per_mtok";
const INTEGER_WIRE_ERROR = "Catalogue prices must be JSON integers or null";

/** The reviver source preserves int64 digits that JSON's number value cannot. */
export async function readCatalogueBody(response: Response): Promise<unknown> {
  return JSON.parse(await response.text(), (key: string, value: unknown, context?: { source?: string }) => {
    if (!key.endsWith(NANOS_SUFFIX) || value === null) return value;
    if (typeof value !== "number" || context?.source === undefined) throw new TypeError(INTEGER_WIRE_ERROR);
    return nanosSchema.parse(context.source);
  });
}

export function stringifyCatalogueBody(body: unknown): string {
  return JSON.stringify(body, (key: string, value: unknown) => {
    if (!key.endsWith(NANOS_SUFFIX) || value === null) return value;
    return sourceJSON.rawJSON(nanosSchema.parse(value));
  });
}

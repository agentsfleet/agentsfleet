// The workspace's model catalogue as `GET W/models` serves it, and the label a
// fleet's model reads by.

import { z } from "zod";
import { nanosSchema } from "./nanos";

export const ModelsSchema = z.object({items: z.array(z.object({
  ref: z.string(), label: z.string(), provider: z.string(), model: z.string(),
  is_default: z.boolean().optional(),
  context_window_tokens: z.number().int().positive(), max_output_tokens: z.number().int().positive().optional(),
  eligibility: z.object({runtime: z.boolean()}),
  pricing: z.object({input_nanos_per_million: nanosSchema, output_nanos_per_million: nanosSchema, runtime_nanos_per_second: nanosSchema}),
})), next_cursor: z.string().nullable()});

export type WorkspaceModel = z.infer<typeof ModelsSchema>["items"][number];

/** The catalogue's label for a fleet's model, or its ref when the catalogue no longer lists it. */
export function runtimeModelLabel(models: readonly WorkspaceModel[] | null, ref: string): string {
  return models?.find(model => model.ref === ref)?.label ?? ref;
}

import { z } from "zod";

export const CONTEXT_STATE = { summarizing: "summarizing", resuming: "resuming", interrupted: "interrupted" } as const;
export const ContextProgressSchema = z.object({
  generation: z.number().int().positive().safe(), state: z.enum(CONTEXT_STATE),
}).strict();
export type ContextProgress = z.infer<typeof ContextProgressSchema>;

/** A duplicate or late start cannot replace a confirmed ending. */
export function advancesContext(previous: ContextProgress | undefined, next: ContextProgress): boolean {
  return previous === undefined || next.generation > previous.generation
    || (next.generation === previous.generation && previous.state === CONTEXT_STATE.summarizing
      && next.state !== CONTEXT_STATE.summarizing);
}

export function readContextProgress(value: unknown): ContextProgress | undefined {
  const parsed = ContextProgressSchema.safeParse(value);
  return parsed.success ? parsed.data : undefined;
}

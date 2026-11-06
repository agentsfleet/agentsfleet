// The browser consumes questions through a checked shape, without replacing native policy.

import { z } from "zod";

const encoder = new TextEncoder();
const TextSchema = z.string().min(1).max(512).refine((text) =>
  encoder.encode(text).byteLength <= 512 && text.trim().length > 0 && !text.includes("\0"));
const NoulQuestionSchema = z.object({question: TextSchema, criteria: z.object({true: TextSchema, false: TextSchema}).strict()}).strict();
const StatementSchema = NoulQuestionSchema.extend({id: z.string().min(1).max(64).regex(/^[A-Za-z0-9_.-]+$/)});
const DecisionPolicySchema = z.object({
  routing: z.object({
    intent: z.object({question: TextSchema, criteria: z.object({
      conversation: TextSchema, status: TextSchema, refresh: TextSchema,
      steer: TextSchema, new_work: TextSchema, clarify: TextSchema,
    }).strict()}).strict(),
    association: z.object({question: TextSchema, criteria: z.object({
      matching_task: TextSchema, new_work: TextSchema, unclear: TextSchema,
    }).strict()}).strict(),
    multiple_requests: NoulQuestionSchema,
  }).strict(),
  completion: z.object({
    statements: z.array(StatementSchema).min(1).max(6)
      .refine((statements) => new Set(statements.map((statement) => statement.id)).size === statements.length),
    next_action: z.object({question: TextSchema, criteria: z.object({
      CONTINUE: TextSchema, WAIT: TextSchema, DONE: TextSchema, BLOCKED: TextSchema,
    }).strict()}).strict(),
  }).strict(),
}).strict();
const ConfigSchema = z.object({"x-agentsfleet": z.object({decision_policy: z.unknown().optional()})});

export type DecisionPolicy = z.infer<typeof DecisionPolicySchema>;
export type DecisionPolicyRead = {kind: "absent"} | {kind: "invalid"} | {kind: "ready"; policy: DecisionPolicy};

export function readDecisionPolicy(config: unknown): DecisionPolicyRead {
  const root = ConfigSchema.safeParse(config);
  if (!root.success) return {kind: "invalid"};
  const runtime = root.data["x-agentsfleet"];
  if (!Object.hasOwn(runtime, "decision_policy")) return {kind: "absent"};
  const result = DecisionPolicySchema.safeParse(runtime.decision_policy);
  return result.success ? {kind: "ready", policy: result.data} : {kind: "invalid"};
}

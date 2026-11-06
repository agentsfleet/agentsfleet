// Readable owner-reviewed questions; native setup remains the validation owner.

import type { Output } from "../../services/output.ts";
import { Effect, Schema } from "effect";

type Question = {readonly question: string; readonly criteria: {readonly true: string; readonly false: string}};

const encoder = new TextEncoder();
const TextSchema = Schema.String.check(Schema.makeFilter((text: string) =>
  encoder.encode(text).byteLength <= 512 && text.trim().length > 0 && !text.includes("\0")));
const NoulCriteriaSchema = Schema.Struct({true: TextSchema, false: TextSchema});
const NoulQuestionSchema = Schema.Struct({question: TextSchema, criteria: NoulCriteriaSchema});
const StatementSchema = Schema.Struct({id: Schema.String.check(Schema.isPattern(/^[A-Za-z0-9_.-]+$/), Schema.isMaxLength(64)),
  ...NoulQuestionSchema.fields});
const DecisionPolicySchema = Schema.Struct({
  routing: Schema.Struct({
    intent: Schema.Struct({question: TextSchema, criteria: Schema.Struct({
      conversation: TextSchema, status: TextSchema, refresh: TextSchema,
      steer: TextSchema, new_work: TextSchema, clarify: TextSchema,
    })}),
    association: Schema.Struct({question: TextSchema, criteria: Schema.Struct({
      matching_task: TextSchema, new_work: TextSchema, unclear: TextSchema,
    })}),
    multiple_requests: NoulQuestionSchema,
  }),
  completion: Schema.Struct({
    statements: Schema.Array(StatementSchema).check(Schema.isMinLength(1), Schema.isMaxLength(6),
      Schema.makeFilter((statements: ReadonlyArray<typeof StatementSchema.Type>) =>
        new Set(statements.map((statement) => statement.id)).size === statements.length)),
    next_action: Schema.Struct({question: TextSchema, criteria: Schema.Struct({
      CONTINUE: TextSchema, WAIT: TextSchema, DONE: TextSchema, BLOCKED: TextSchema,
    })}),
  }),
});
const ConfigSchema = Schema.Struct({"x-agentsfleet": Schema.Struct({decision_policy: Schema.optionalKey(Schema.Unknown)})});
const decodePolicy = Schema.decodeUnknownEffect(DecisionPolicySchema, {onExcessProperty: "error"});
const UNREADABLE = "Success criteria could not be read. Review the saved setup with a matching CLI build.";

export function renderQuestions(output: Output, config: unknown): Effect.Effect<void> {
  return Schema.decodeUnknownEffect(ConfigSchema)(config).pipe(
    Effect.flatMap((decoded) => {
      const policy = decoded["x-agentsfleet"].decision_policy;
      return policy === undefined ? Effect.succeed(lines(undefined)) : decodePolicy(policy).pipe(Effect.map(lines));
    }),
    Effect.catch(() => Effect.succeed([UNREADABLE])),
    Effect.flatMap((messages) => Effect.forEach(messages, (message) => output.info(message), {discard: true})),
  );
}

/** Questions have their own display, so the remaining config shows each fact once. */
export function remainingConfig(config: Readonly<Record<string, unknown>>): Readonly<Record<string, unknown>> {
  const runtime = config["x-agentsfleet"];
  if (typeof runtime !== "object" || runtime === null || Array.isArray(runtime)) return config;
  return {...config, "x-agentsfleet": Object.fromEntries(Object.entries(runtime).filter(([key]) => key !== "decision_policy"))};
}

function lines(policy: typeof DecisionPolicySchema.Type | undefined): string[] {
  if (!policy) return [];
  return [
    "What counts as done?",
    ...policy.completion.statements.map((statement, index) => formatQuestion(`${index + 1}.`, statement)),
    "Change these criteria through setup chat. Saving creates a new revision. Work already started keeps its saved success criteria.",
  ];
}

function formatQuestion(label: string, question: Question): string {
  return [`${label} ${question.question}`, `  Yes: ${question.criteria.true}`, `  No: ${question.criteria.false}`].join("\n");
}

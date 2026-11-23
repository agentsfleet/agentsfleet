import {Schema} from "effect";
import {instant} from "../../lib/instant.ts";

const text = Schema.String.check(Schema.isMinLength(1), Schema.isPattern(/^[^\0]*$/u));
const bps = Schema.Number.check(Schema.isInt(), Schema.isBetween({minimum: 0, maximum: 10000}));
const threshold = Schema.Struct({probability_bps: bps, confidence_bps: bps});
const intent = Schema.Struct({
  questions_revision: Schema.Literal("dt-routing-v1"), intent: threshold, association: threshold,
  single_request_max_bps: bps, multiple_request_min_bps: bps,
}).check(Schema.makeFilter(row => row.single_request_max_bps < row.multiple_request_min_bps));
const goal = Schema.Struct({
  questions_revision: Schema.Literal("dt-goal-v1"), statement_min_bps: bps,
  continue_action: threshold, wait: threshold, done: threshold,
});
export const DecisionProfileSchema = Schema.Struct({
  revision: text.check(Schema.isPattern(/^[A-Za-z0-9_.:-]+$/u)),
  api: Schema.Literals(["typesafe", "openai", "perplexity", "cloudflare"]),
  endpoint: text, accepted_models: Schema.Array(text).check(Schema.isMinLength(1)),
  capabilities: Schema.Struct({noul: Schema.Boolean, choice: Schema.Boolean, score: Schema.Boolean}),
  input_max_bytes: Schema.Number.check(Schema.isInt(), Schema.isGreaterThanOrEqualTo(1)),
  intent, goal, evaluation_digest: Schema.String.check(Schema.isPattern(/^[0-9a-f]{64}$/u)),
  evaluation_source_url: text, evaluated_at: instant,
});

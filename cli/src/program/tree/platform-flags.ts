import { Flag } from "effect/cli";
import { instant } from "../../lib/instant.ts";
import { Schema } from "effect";
import { boundedInt, METAVAR, requiredTextFlag, startingAfterFlag } from "./flags.ts";

const PAGE_MIN = 1;
const PAGE_MAX = 100;
const INSTANT_DETAIL = "expected an RFC 3339 timestamp";

export const platformInstantFlag = (name: string) => requiredTextFlag(name, "RFC 3339 event time", METAVAR.when).pipe(
  Flag.filter(Schema.is(instant), () => INSTANT_DETAIL), Flag.optional,
);

export const platformPagingFlags = {
  limit: boundedInt("limit", PAGE_MIN, PAGE_MAX).pipe(
    Flag.withDescription("Page size supported by the API (1–100)"), Flag.optional,
  ),
  startingAfter: startingAfterFlag,
};

export const platformFileFlag = requiredTextFlag("file", "JSON input file; - reads stdin", METAVAR.path);
export const platformPolicyFlag = requiredTextFlag("policy", "Complete assigned runner policy JSON file; - reads stdin", METAVAR.path);
export const platformLabelsFlag = requiredTextFlag("labels", "Comma-separated tags; an empty value clears labels", METAVAR.text).pipe(Flag.optional);
export const platformTokenFlag = requiredTextFlag("token-out", "New private file for the one-time runner token (mode 0600)", METAVAR.path);
export const platformEtagFlag = requiredTextFlag("if-match", "Exact inspected resource tag returned by list/show", METAVAR.etag);

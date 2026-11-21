import { Flag } from "effect/cli";

import { METAVAR, positiveInt, requiredTextFlag, startingAfterFlag } from "./flags.ts";

// The scope each platform leaf needs, in the server's claim spelling. Help
// shows a leaf only to a caller whose server-reported scopes include it; the
// server still decides every call.
export const PLATFORM_SCOPE = {
  keyRead: "platform-key:read",
  keyAdmin: "platform-key:admin",
  modelRead: "model:read",
  modelAdmin: "model:admin",
  runnerRead: "runner:read",
  runnerRegister: "runner:register",
  runnerWrite: "runner:write",
} as const;

export const platformPagingFlags = {
  limit: positiveInt("limit").pipe(
    Flag.withDescription("Page size"), Flag.optional,
  ),
  startingAfter: startingAfterFlag,
};

export const platformFileFlag = requiredTextFlag("file", "JSON input file; - reads stdin", METAVAR.path);
export const platformPolicyFlag = requiredTextFlag("policy", "Complete assigned runner policy JSON file; - reads stdin", METAVAR.path);
export const platformLabelsFlag = requiredTextFlag("labels", "Comma-separated tags; an empty value clears labels", METAVAR.text).pipe(Flag.optional);
export const platformTokenFlag = requiredTextFlag("token-out", "New private file for the one-time runner token (mode 0600)", METAVAR.path);
export const platformEtagFlag = requiredTextFlag("if-match", "Exact inspected resource tag returned by list/show", METAVAR.etag);

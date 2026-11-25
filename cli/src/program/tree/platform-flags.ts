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
    Flag.withDescription("How many to show"), Flag.optional,
  ),
  startingAfter: startingAfterFlag,
};

export const platformFileFlag = requiredTextFlag("file", "JSON file to read (- reads stdin)", METAVAR.path);
export const platformPolicyFlag = requiredTextFlag("policy", "The runner's full settings as a JSON file (- reads stdin)", METAVAR.path);
export const platformLabelsFlag = requiredTextFlag("labels", "Labels, separated by commas; an empty value clears them", METAVAR.text).pipe(Flag.optional);
export const platformTokenFlag = requiredTextFlag("token-out", "A new file, readable only by you, to save the runner's token in (shown once)", METAVAR.path);
export const platformEtagFlag = requiredTextFlag("if-match", "The version tag list or show printed, so you never overwrite a newer change", METAVAR.etag);

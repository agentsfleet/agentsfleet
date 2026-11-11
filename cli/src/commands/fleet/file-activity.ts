import { diffLines } from "diff";
import { isRecord, isString } from "../../lib/guards.ts";
import { activityText } from "./task-activity.ts";

/** Recorded edit inputs are a local diff, never proof of repository publication. */
export function fileActivity(name: string, args: unknown, status: string): string | undefined {
  if (!isRecord(args)) return undefined;
  const label = status === "succeeded" ? "Confirmed local edit" : "Requested local edit";
  if (name === "apply_patch" && isString(args["patch"])) {
    return `${label} · not a publication\n${activityText(args["patch"])}`;
  }
  if (name !== "file_edit" || !isString(args["path"]) || !isString(args["old_text"]) || !isString(args["new_text"])) return undefined;
  // Bound both the input and edit search, as the chat diff renderer does.
  const before = activityText(args["old_text"]);
  const after = activityText(args["new_text"]);
  const changes = diffLines(before, after, {maxEditLength: 1000, ignoreNewlineAtEof: true})
    ?? [{value: before, removed: true}, {value: after, added: true}];
  const rows = changes.flatMap(change => {
    const prefix = change.added ? "+ " : change.removed ? "- " : "  ";
    return change.value.replace(/\n$/u, "").split("\n").map(line => `${prefix}${line}`);
  });
  return activityText(`${label}: ${args["path"]} · not a publication\n${rows.join("\n")}`);
}

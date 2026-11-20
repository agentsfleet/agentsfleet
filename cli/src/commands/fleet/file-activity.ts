import { isNumber, isRecord, isString } from "../../lib/guards.ts";
import { activityText } from "./task-activity.ts";

/** The tools whose recorded changes are an edit's files and line counts. */
const EDIT_TOOLS: ReadonlySet<string> = new Set(["file_write", "file_append", "file_edit", "file_edit_hashed", "apply_patch"]);
const SUCCEEDED = "succeeded";
const MINUS = "−";

/** One file an edit changed, as the runner's file tool counted it. */
type FileChange = { path: string; added: number; removed: number };

const isCount = (value: unknown): value is number => isNumber(value) && Number.isSafeInteger(value) && value >= 0;

/** Every recorded change, or none when any entry is not one: a count the CLI
 * cannot vouch for is not printed. */
function readChanges(value: unknown): FileChange[] {
  if (!Array.isArray(value)) return [];
  const changes: FileChange[] = [];
  for (const entry of value) {
    if (!isRecord(entry) || !isString(entry["path"]) || entry["path"] === "" || !isCount(entry["added"]) || !isCount(entry["removed"])) return [];
    changes.push({ path: entry["path"], added: entry["added"], removed: entry["removed"] });
  }
  return changes;
}

/** A succeeded edit's files and line counts, read from the changes the runner
 * recorded, never worked out from its arguments. A local edit, never proof of
 * repository publication. */
export function fileActivity(name: string, changes: unknown, status: string): string | undefined {
  if (!EDIT_TOOLS.has(name) || status !== SUCCEEDED) return undefined;
  const files = readChanges(changes);
  if (files.length === 0) return undefined;
  const rows = files.map(file => `${file.path} +${file.added} ${MINUS}${file.removed}`);
  return activityText(`Confirmed local edit · not a publication\n${rows.join("\n")}`);
}

import type { RecordedChangeRepository } from "@/components/domain/conversation/summary/recorded-changes";
import { DIFF_ROW, lineRows } from "@/components/domain/activity/tools/diff/tool-call-diff";

const WEB = {id: "reference:web", label: "web", fullName: "reference/web"};
const DOCS = {id: "reference:docs", label: "docs", fullName: "reference/docs"};
const sample = (count: number) => Array.from({length: count}, (_, index) => `Recorded reference line ${index + 1}`).join("\n");

/** Complete mock index data for the approved example, not inferred from the paths. */
export const LAUNCH_CHANGE_FILES = [
  {path: "src/components/LaunchPage.tsx", repository: WEB, old_text: "const compact = true;", new_text: sample(124)},
  {path: "src/styles/theme.css", repository: WEB, old_text: "min-height: 32px;", new_text: "min-height: 44px;"},
  {path: "src/components/MobileNav.tsx", repository: WEB, old_text: "const cramped = true;", new_text: sample(83)},
  {path: "src/components/UsageTable.tsx", repository: WEB, old_text: "<table>", new_text: `<table>\n${sample(40)}`},
  {path: "design-review.md", repository: DOCS, old_text: "# Design review", new_text: `# Design review\n${sample(14)}`},
  {path: "launch-checklist.md", repository: DOCS, old_text: "# Launch checklist", new_text: `# Launch checklist\n${sample(9)}`},
] as const;

/** Added and removed line counts of one reference edit. */
function lineCounts(before: string, after: string): {added: number; removed: number} {
  const rows = lineRows(before, after);
  return {added: rows.filter(row => row.kind === DIFF_ROW.ADDED).length, removed: rows.filter(row => row.kind === DIFF_ROW.REMOVED).length};
}

export function launchChangeRepositories(turnId: string, webLabel?: string | null): RecordedChangeRepository[] {
  return LAUNCH_CHANGE_FILES.map(file => {
    const {added, removed} = lineCounts(file.old_text, file.new_text);
    const repository = webLabel && file.repository === WEB ? {...WEB, label: webLabel, fullName: webLabel} : file.repository;
    return {turnId, path: file.path, repository, lines: {added, removed}};
  });
}

export function launchEditArguments(index: number) {
  const file = LAUNCH_CHANGE_FILES[index];
  if (!file) throw new Error("Unknown launch-review reference file");
  return {path: file.path, edits: [{old_text: file.old_text, new_text: file.new_text}]};
}

/** The runner's recorded line counts for one reference edit, as its answer carries them. */
export function launchEditChanges(index: number) {
  const file = LAUNCH_CHANGE_FILES[index];
  if (!file) throw new Error("Unknown launch-review reference file");
  const {added, removed} = lineCounts(file.old_text, file.new_text);
  return [{path: file.path, added, removed}];
}

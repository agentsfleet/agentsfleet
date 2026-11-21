import type { RecordedChangeRepository } from "@/components/domain/conversation/summary/recorded-changes";
import { lineDiff } from "@/components/domain/activity/tools/diff/tool-call-diff";

const WEB = {id: "reference:web", label: "web", fullName: "reference/web"};
const DOCS = {id: "reference:docs", label: "docs", fullName: "reference/docs"};
const added = (count: number) => Array.from({length: count}, (_, index) => `Recorded reference line ${index + 1}`).join("\n");

/** Complete mock index data for the approved example, not inferred from the paths. */
export const LAUNCH_CHANGE_FILES = [
  {path: "src/components/LaunchPage.tsx", repository: WEB, old_text: "const compact = true;", new_text: added(124)},
  {path: "src/styles/theme.css", repository: WEB, old_text: "min-height: 32px;", new_text: "min-height: 44px;"},
  {path: "src/components/MobileNav.tsx", repository: WEB, old_text: "const cramped = true;", new_text: added(83)},
  {path: "src/components/UsageTable.tsx", repository: WEB, old_text: "<table>", new_text: `<table>\n${added(40)}`},
  {path: "design-review.md", repository: DOCS, old_text: "# Design review", new_text: `# Design review\n${added(14)}`},
  {path: "launch-checklist.md", repository: DOCS, old_text: "# Launch checklist", new_text: `# Launch checklist\n${added(9)}`},
] as const;

export function launchChangeRepositories(eventId: string): RecordedChangeRepository[] {
  return LAUNCH_CHANGE_FILES.map(file => {
    const {added, removed} = lineDiff(file.old_text, file.new_text);
    return {eventId, path: file.path, repository: file.repository, lines: {added, removed}};
  });
}

export function launchEditArguments(index: number) {
  const file = LAUNCH_CHANGE_FILES[index];
  if (!file) throw new Error("Unknown launch-review reference file");
  return {path: file.path, old_text: file.old_text, new_text: file.new_text};
}

/** Leading characters shown for a workspace id; the full id rides the title. */
const WORKSPACE_ID_DISPLAY_CHARS = 8;
const TRUNCATION_ELLIPSIS = "…";

export type LeaseFilters = {
  /** The workspace id the feed is narrowed to, or null when unfiltered. */
  workspace: string | null;
  /** The fleet id or exact name the feed is narrowed to, or null. */
  fleet: string | null;
};

export function shortWorkspaceId(workspaceId: string): string {
  return workspaceId.length > WORKSPACE_ID_DISPLAY_CHARS
    ? `${workspaceId.slice(0, WORKSPACE_ID_DISPLAY_CHARS)}${TRUNCATION_ELLIPSIS}`
    : workspaceId;
}

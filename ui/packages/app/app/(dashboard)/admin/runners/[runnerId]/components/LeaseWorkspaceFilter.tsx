"use client";

import { useCallback } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { XIcon } from "lucide-react";
import { Badge, IconAction } from "@agentsfleet/design-system";
import {
  CURSOR_PAGE_SIZE_PARAM,
  CURSOR_TRAIL_PARAM,
} from "@/lib/pagination/cursor-trail";
import {
  CLEAR_WORKSPACE_FILTER_LABEL,
  WORKSPACE_FILTER_PARAM,
  WORKSPACE_LABEL,
} from "./runner-copy";

// The client half of the lease workspace filter, mirroring the cursor pager's
// split: the hook WRITES the `workspace` search param, the Server Component
// reads it back and fetches the filtered page. A filter change names a
// different result set, so the cursor trail walked through the old one is
// dropped in the same navigation — its cursors cannot page the new feed.

/** Leading characters shown for a workspace id; the full id rides the title. */
const WORKSPACE_ID_DISPLAY_CHARS = 8;
const TRUNCATION_ELLIPSIS = "…";

export function shortWorkspaceId(workspaceId: string): string {
  return workspaceId.length > WORKSPACE_ID_DISPLAY_CHARS
    ? `${workspaceId.slice(0, WORKSPACE_ID_DISPLAY_CHARS)}${TRUNCATION_ELLIPSIS}`
    : workspaceId;
}

export type LeaseWorkspaceFilterState = {
  /** The workspace id the URL filters to, or null when unfiltered. */
  active: string | null;
  filterTo: (workspaceId: string) => void;
  clear: () => void;
};

export function useLeaseWorkspaceFilter(): LeaseWorkspaceFilterState {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const raw = params.get(WORKSPACE_FILTER_PARAM);
  const active = raw !== null && raw.length > 0 ? raw : null;

  const apply = useCallback(
    (workspaceId: string | null) => {
      // Rebuilt from the live params so every other query value survives the
      // navigation, exactly as the pager's page turn does — except the trail,
      // which resets with the result set it walked.
      const next = new URLSearchParams(params.toString());
      next.delete(CURSOR_TRAIL_PARAM);
      next.delete(CURSOR_PAGE_SIZE_PARAM);
      if (workspaceId === null) next.delete(WORKSPACE_FILTER_PARAM);
      else next.set(WORKSPACE_FILTER_PARAM, workspaceId);
      const query = next.toString();
      router.push(query.length > 0 ? `${pathname}?${query}` : pathname, {
        scroll: true,
      });
    },
    [params, pathname, router],
  );

  const filterTo = useCallback(
    (workspaceId: string) => apply(workspaceId),
    [apply],
  );
  const clear = useCallback(() => apply(null), [apply]);

  return { active, filterTo, clear };
}

// The active-filter chip: names the workspace the feed is narrowed to and
// clears back to the unfiltered feed. The id keeps its real casing — a badge's
// default uppercase would misquote it.
export function LeaseWorkspaceFilter({
  workspaceId,
  onClear,
}: {
  workspaceId: string;
  onClear: () => void;
}) {
  return (
    <div className="mb-lg flex items-center gap-md">
      <Badge className="normal-case" title={workspaceId}>
        {WORKSPACE_LABEL} {shortWorkspaceId(workspaceId)}
      </Badge>
      <IconAction label={CLEAR_WORKSPACE_FILTER_LABEL} onClick={onClear}>
        <XIcon />
      </IconAction>
    </div>
  );
}

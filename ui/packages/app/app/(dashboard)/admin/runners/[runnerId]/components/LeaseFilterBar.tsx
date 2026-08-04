"use client";

import { useCallback, useEffect, useState } from "react";
import {
  usePathname,
  useRouter,
  useSearchParams,
  type ReadonlyURLSearchParams,
} from "next/navigation";
import { XIcon } from "lucide-react";
import { Badge, Button, IconAction, Input, Label } from "@agentsfleet/design-system";
import { CURSOR_PAGE_SIZE_PARAM, CURSOR_TRAIL_PARAM } from "@/lib/pagination/cursor-trail";
import {
  formatLeaseFilterQuery,
  parseLeaseFilterQuery,
  shortWorkspaceId,
  type LeaseFilters,
} from "./lease-filter-query";
import {
  APPLY_LEASE_FILTER_LABEL,
  CLEAR_FLEET_FILTER_LABEL,
  CLEAR_LEASE_FILTER_LABEL,
  CLEAR_WORKSPACE_FILTER_LABEL,
  FLEET_FILTER_PARAM,
  FLEET_LABEL,
  LEASE_FILTER_HINT,
  LEASE_FILTER_LABEL,
  LEASE_FILTER_PLACEHOLDER,
  WORKSPACE_FILTER_PARAM,
  WORKSPACE_LABEL,
} from "./runner-copy";

// The client half of the lease filters, mirroring the cursor pager's split: the
// hook WRITES the search params, the Server Component reads them back and fetches
// the filtered page. A filter change names a different result set, so the cursor
// trail walked through the old one is dropped in the same navigation — its
// cursors cannot page the new feed.

const FILTER_INPUT_ID = "lease-filter-query";

export type LeaseFilterState = LeaseFilters & {
  apply: (filters: LeaseFilters) => void;
  clearWorkspace: () => void;
  clearFleet: () => void;
  clearAll: () => void;
};

/** An absent param and a present-but-empty one both mean unfiltered. */
function readParam(params: ReadonlyURLSearchParams, name: string): string | null {
  const raw = params.get(name);
  return raw !== null && raw.length > 0 ? raw : null;
}

export function useLeaseFilters(): LeaseFilterState {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const workspace = readParam(params, WORKSPACE_FILTER_PARAM);
  const fleet = readParam(params, FLEET_FILTER_PARAM);

  const apply = useCallback(
    (next: LeaseFilters) => {
      // Rebuilt from the live params so every other query value survives the
      // navigation, exactly as the pager's page turn does — except the trail,
      // which resets with the result set it walked.
      const search = new URLSearchParams(params.toString());
      search.delete(CURSOR_TRAIL_PARAM);
      search.delete(CURSOR_PAGE_SIZE_PARAM);
      if (next.workspace === null) search.delete(WORKSPACE_FILTER_PARAM);
      else search.set(WORKSPACE_FILTER_PARAM, next.workspace);
      if (next.fleet === null) search.delete(FLEET_FILTER_PARAM);
      else search.set(FLEET_FILTER_PARAM, next.fleet);
      const query = search.toString();
      router.push(query.length > 0 ? `${pathname}?${query}` : pathname, { scroll: true });
    },
    [params, pathname, router],
  );

  const clearWorkspace = useCallback(() => apply({ workspace: null, fleet }), [apply, fleet]);
  const clearFleet = useCallback(() => apply({ workspace, fleet: null }), [apply, workspace]);
  const clearAll = useCallback(() => apply({ workspace: null, fleet: null }), [apply]);

  return { workspace, fleet, apply, clearWorkspace, clearFleet, clearAll };
}

// The toolbar: one input carrying the whole query, plus a chip per active filter
// so the operator can drop one without re-typing the other.
export function LeaseFilterBar({ filters }: { filters: LeaseFilterState }) {
  const applied = formatLeaseFilterQuery(filters);
  const [draft, setDraft] = useState(applied);

  // The URL is the source of truth: a back/forward navigation, or a chip's
  // clear, changes the applied query without touching this input, so the draft
  // follows it rather than stranding the operator's view out of sync.
  useEffect(() => setDraft(applied), [applied]);

  const submit = useCallback(() => filters.apply(parseLeaseFilterQuery(draft)), [draft, filters]);

  const hasActiveFilter = filters.workspace !== null || filters.fleet !== null;

  return (
    <div className="mb-lg flex flex-col gap-md">
      <Label htmlFor={FILTER_INPUT_ID}>{LEASE_FILTER_LABEL}</Label>
      <div className="flex flex-wrap items-center gap-md">
        <Input
          id={FILTER_INPUT_ID}
          className="min-w-measure flex-1"
          value={draft}
          placeholder={LEASE_FILTER_PLACEHOLDER}
          aria-describedby={`${FILTER_INPUT_ID}-hint`}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              submit();
            }
          }}
        />
        <Button type="button" onClick={submit}>
          {APPLY_LEASE_FILTER_LABEL}
        </Button>
        {hasActiveFilter ? (
          <Button type="button" variant="ghost" onClick={filters.clearAll}>
            {CLEAR_LEASE_FILTER_LABEL}
          </Button>
        ) : null}
      </div>
      <p id={`${FILTER_INPUT_ID}-hint`} className="text-label text-text-subtle">
        {LEASE_FILTER_HINT}
      </p>
      {hasActiveFilter ? (
        <div className="flex flex-wrap items-center gap-md">
          {filters.workspace !== null ? (
            <FilterChip
              label={WORKSPACE_LABEL}
              value={shortWorkspaceId(filters.workspace)}
              title={filters.workspace}
              clearLabel={CLEAR_WORKSPACE_FILTER_LABEL}
              onClear={filters.clearWorkspace}
            />
          ) : null}
          {filters.fleet !== null ? (
            <FilterChip
              label={FLEET_LABEL}
              value={filters.fleet}
              title={filters.fleet}
              clearLabel={CLEAR_FLEET_FILTER_LABEL}
              onClear={filters.clearFleet}
            />
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

// The id and name keep their real casing — a badge's default uppercase would
// misquote both.
function FilterChip({
  label,
  value,
  title,
  clearLabel,
  onClear,
}: {
  label: string;
  value: string;
  title: string;
  clearLabel: string;
  onClear: () => void;
}) {
  return (
    <span className="flex items-center gap-md">
      <Badge className="normal-case" title={title}>
        {label} {value}
      </Badge>
      <IconAction label={clearLabel} onClick={onClear}>
        <XIcon />
      </IconAction>
    </span>
  );
}

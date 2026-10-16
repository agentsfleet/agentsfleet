"use client";

import { useCallback, useEffect, useState, type KeyboardEvent } from "react";
import {
  usePathname,
  useRouter,
  useSearchParams,
  type ReadonlyURLSearchParams,
} from "next/navigation";
import { FilterIcon, XIcon } from "lucide-react";
import { Badge, Button, IconAction, Input, Label } from "@agentsfleet/design-system";
import { CURSOR_PAGE_SIZE_PARAM, CURSOR_TRAIL_PARAM } from "@/lib/pagination/cursor-trail";
import {
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
  WORKSPACE_FILTER_PARAM,
  WORKSPACE_LABEL,
} from "./runner-copy";

// The client half of the lease filters, mirroring the cursor pager's split: the
// hook WRITES the search params, the Server Component reads them back and fetches
// the filtered page. A filter change names a different result set, so the cursor
// trail walked through the old one is dropped in the same navigation — its
// cursors cannot page the new feed.

const FLEET_INPUT_ID = "lease-fleet-filter";
const WORKSPACE_INPUT_ID = "lease-workspace-filter";
const FILTER_HINT_ID = "lease-filter-hint";

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

// Separate fields preserve literal fleet names, including spaces and colons.
export function LeaseFilterBar({ filters }: { filters: LeaseFilterState }) {
  const [fleet, setFleet] = useState(filters.fleet ?? "");
  const [workspace, setWorkspace] = useState(filters.workspace ?? "");
  useEffect(() => {
    setFleet(filters.fleet ?? "");
    setWorkspace(filters.workspace ?? "");
  }, [filters.fleet, filters.workspace]);

  const submit = () => filters.apply({ fleet: fleet.trim() || null, workspace: workspace.trim() || null });
  const submitOnEnter = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    submit();
  };

  const hasActiveFilter = filters.workspace !== null || filters.fleet !== null;

  return (
    <div className="mb-lg flex flex-col gap-md">
      <div className="flex flex-wrap items-end gap-lg">
        <div className="flex w-full min-w-0 flex-col gap-sm sm:w-auto sm:flex-1">
          <Label htmlFor={FLEET_INPUT_ID}>{FLEET_LABEL}</Label>
          <Input id={FLEET_INPUT_ID} value={fleet} placeholder="Name or ID"
            aria-describedby={FILTER_HINT_ID} onChange={(event) => setFleet(event.target.value)} onKeyDown={submitOnEnter} />
        </div>
        <div className="flex w-full min-w-0 flex-col gap-sm sm:w-auto sm:flex-1">
          <Label htmlFor={WORKSPACE_INPUT_ID}>{WORKSPACE_LABEL}</Label>
          <Input id={WORKSPACE_INPUT_ID} value={workspace} placeholder="Workspace ID"
            aria-describedby={FILTER_HINT_ID} onChange={(event) => setWorkspace(event.target.value)} onKeyDown={submitOnEnter} />
        </div>
        <Button type="button" variant="outline" onClick={submit}>
          <FilterIcon aria-hidden="true" /> {APPLY_LEASE_FILTER_LABEL}
        </Button>
        {hasActiveFilter ? (
          <Button type="button" variant="ghost" onClick={filters.clearAll}>
            {CLEAR_LEASE_FILTER_LABEL}
          </Button>
        ) : null}
      </div>
      <p id={FILTER_HINT_ID} className="text-label text-text-subtle">
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
      <IconAction variant="ghost" label={clearLabel} onClick={onClear}>
        <XIcon />
      </IconAction>
    </span>
  );
}

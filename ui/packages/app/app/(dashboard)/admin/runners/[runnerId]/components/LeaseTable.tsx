"use client";

import { useMemo, useState } from "react";
import { LayoutListIcon } from "lucide-react";
import { DataTable, EmptyState, PAGINATION_KIND } from "@agentsfleet/design-system";
import type { RunnerLease, RunnerLeaseResponse } from "@/lib/api/runners";
import { LEASE_OUTCOME } from "@/lib/api/runners-types";
import { TABLE_PAGE_SIZE_OPTIONS } from "@/lib/pagination/cursor-trail";
import { useUrlCursorPages } from "@/lib/pagination/use-url-cursor-pages";
import { LeaseFilterBar, useLeaseFilters } from "./LeaseFilterBar";
import { leaseColumns } from "./lease-columns";
import { ReviewLease } from "./ReviewLease";
import { LEASES_EMPTY_DESCRIPTION, LEASES_EMPTY_TITLE, LEASES_TABLE_LABEL } from "./runner-copy";

// The server reads one filtered page; this component owns selection and paging.
export function LeaseTable({initial, pageSize}: {initial: RunnerLeaseResponse; pageSize: number}) {
  const [selected, setSelected] = useState<RunnerLease | null>(null);
  const feed = useUrlCursorPages(initial.next_cursor, pageSize);
  const filters = useLeaseFilters();
  const columns = useMemo(() => leaseColumns(setSelected), [setSelected]);
  // Live work leads within the fetched page; each group keeps server order.
  const rows = useMemo(() => [
    ...initial.items.filter((lease) => lease.outcome === LEASE_OUTCOME.running),
    ...initial.items.filter((lease) => lease.outcome !== LEASE_OUTCOME.running),
  ], [initial.items]);

  return <div className="flex min-h-0 flex-1 flex-col">
    <LeaseFilterBar filters={filters} />
    <DataTable caption={LEASES_TABLE_LABEL} columns={columns} rows={rows} rowKey={(lease) => lease.id}
      pagination={{
        kind: PAGINATION_KIND.page, page: feed.page, pageSize, hasNext: feed.hasNext,
        total: initial.total ?? undefined, totalLabel: "leases", onPageChange: feed.goToPage,
        pageSizeOptions: TABLE_PAGE_SIZE_OPTIONS, onPageSizeChange: feed.changePageSize, isLoading: feed.isLoading,
      }}
      empty={<EmptyState icon={<LayoutListIcon size={28} />} title={LEASES_EMPTY_TITLE} description={LEASES_EMPTY_DESCRIPTION} />}
    />
    <ReviewLease lease={selected} onOpenChange={() => setSelected(null)} />
  </div>;
}

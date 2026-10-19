"use client";

import { useDeferredValue, useMemo, useRef, useState, useTransition } from "react";
import { DataTable, EmptyState, Section, PAGINATION_KIND, DEFAULT_PAGE_SIZE_OPTIONS, type DataTableColumn } from "@agentsfleet/design-system";
import type { TenantPlatformDefault } from "@/lib/types";
import { CollectionToolbar } from "@/components/domain/CollectionToolbar";
import { rowKey, type RegistryRow } from "./ModelsRegistryCells";
import type { SortState } from "./registry-view";

type Props = {
  rows: RegistryRow[];
  columns: DataTableColumn<RegistryRow>[];
  platformDefault: TenantPlatformDefault | null;
  sort: SortState;
  onSortChange: (key: string) => void;
  pending: boolean;
  onLoadMore?: () => Promise<boolean>;
};

export function ModelsRegistryView({ rows, columns, platformDefault, sort, onSortChange, pending, onLoadMore }: Props) {
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState<number>(DEFAULT_PAGE_SIZE_OPTIONS[0]);
  const [loading, startLoading] = useTransition();
  const pageIntent = useRef(0);
  function resetPage() { pageIntent.current += 1; setPage(1); }
  const query = useDeferredValue(search.trim().toLowerCase());
  const matches = useMemo(() => rows.filter((row) => {
    const label = row.kind === "default" ? `platform default ${platformDefault?.model ?? ""} ${platformDefault?.provider ?? ""}`
      : `${row.entry.model_id} ${row.entry.provider}`;
    return label.toLowerCase().includes(query);
  }), [rows, platformDefault, query]);
  const currentPage = Math.min(page, Math.max(1, Math.ceil(matches.length / pageSize)));
  function changePage(next: number, loadMore: () => Promise<boolean>) {
    if (next <= Math.ceil(matches.length / pageSize)) { setPage(next); return; }
    const intent = pageIntent.current;
    const destination = currentPage * pageSize > matches.length ? currentPage : next;
    startLoading(async () => {
      if (await loadMore() && pageIntent.current === intent) setPage(destination);
    });
  }
  return <Section className="space-y-xl">
    <CollectionToolbar label="Search models" value={search} onChange={(value) => { setSearch(value); resetPage(); }} summary={`${matches.length} of ${rows.length} models`} />
    <DataTable columns={columns} rows={matches.slice((currentPage - 1) * pageSize, currentPage * pageSize)} rowKey={rowKey} caption="Models" isLoading={pending || loading}
      sortKey={sort?.key} sortDirection={sort?.dir} onSortChange={(key) => { resetPage(); onSortChange(key); }}
      pagination={{kind: PAGINATION_KIND.page, page: currentPage, pageSize,
        hasNext: currentPage * pageSize < matches.length || Boolean(onLoadMore),
        total: onLoadMore ? undefined : matches.length, onPageChange: onLoadMore ? (next) => changePage(next, onLoadMore) : setPage,
        pageSizeOptions: DEFAULT_PAGE_SIZE_OPTIONS, onPageSizeChange: (size) => { setPageSize(size); resetPage(); },
        isLoading: pending || loading}}
      empty={<EmptyState title="No matching models" description="Try a provider or model name. Use Next to search more models." />} />
  </Section>;
}

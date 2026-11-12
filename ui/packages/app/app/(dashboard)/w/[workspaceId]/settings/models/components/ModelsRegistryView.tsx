"use client";

import { useDeferredValue, useMemo, useRef, useState, useTransition } from "react";
import { SortAscendingIcon } from "@phosphor-icons/react/ssr";
import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, EmptyState, Pagination, Section, PAGINATION_KIND, DEFAULT_PAGE_SIZE_OPTIONS, type DataTableColumn } from "@agentsfleet/design-system";
import type { TenantPlatformDefault } from "@/lib/types";
import { CollectionToolbar } from "@/components/domain/CollectionToolbar";
import { type RegistryRow } from "./ModelsRegistryCells";
import { ModelsRegistryCards } from "./ModelsRegistryCards";
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
    <div className="flex flex-wrap items-center gap-lg">
      <div className="min-w-0 flex-1 basis-full sm:basis-auto"><CollectionToolbar label="Search models" value={search} onChange={(value) => { setSearch(value); resetPage(); }} summary={`${matches.length} of ${rows.length} models`} /></div>
      <DropdownMenu><DropdownMenuTrigger asChild><Button variant="outline" size="sm" aria-label="Sort models"><SortAscendingIcon size={16} aria-hidden="true" />Sort</Button></DropdownMenuTrigger>
        <DropdownMenuContent align="end">{columns.filter(column => column.sortable).map(column => <DropdownMenuItem key={column.key}
          onSelect={() => { resetPage(); onSortChange(column.key); }}>
          {column.header}{sort?.key === column.key ? ` · ${sort.dir}` : ""}
        </DropdownMenuItem>)}</DropdownMenuContent>
      </DropdownMenu>
    </div>
    <div aria-busy={pending || loading}>
      {matches.length === 0 ? <EmptyState title="No matching models" description="Try a provider or model name. Use Next to search more models." />
        : <ModelsRegistryCards columns={columns} rows={matches.slice((currentPage - 1) * pageSize, currentPage * pageSize)} />}
    </div>
    {matches.length > pageSize || onLoadMore || pageSize !== DEFAULT_PAGE_SIZE_OPTIONS[0] || currentPage > 1 ? <Pagination kind={PAGINATION_KIND.page} page={currentPage} pageSize={pageSize}
      hasNext={currentPage * pageSize < matches.length || Boolean(onLoadMore)}
      total={onLoadMore ? undefined : matches.length} onPageChange={onLoadMore ? next => changePage(next, onLoadMore) : setPage}
      pageSizeOptions={DEFAULT_PAGE_SIZE_OPTIONS} onPageSizeChange={size => { setPageSize(size); resetPage(); }}
      isLoading={pending || loading} /> : null}

  </Section>;
}

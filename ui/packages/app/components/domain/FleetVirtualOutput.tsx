"use client";

import type { KeyboardEvent } from "react";
import type { RowComponentProps } from "react-window";
import type { NumberedRow } from "./FleetOutputLine";
import { useCallback, useMemo, useRef, useState } from "react";
import { List, useDynamicRowHeight, useListRef } from "react-window";
import { Input } from "@agentsfleet/design-system";
import { FleetOutputLine } from "./FleetOutputLine";

const ESTIMATED_LINE_HEIGHT = 24;
const DEFAULT_VIEWPORT_HEIGHT = 384;

/** The library owns windowing and wrapped-line measurement; search covers all retained lines. */
export function FleetVirtualOutput({ rows }: { rows: readonly NumberedRow[] }) {
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => {
    const needle = query.toLocaleLowerCase();
    return needle ? rows.filter((row) => row.text.toLocaleLowerCase().includes(needle)) : rows;
  }, [query, rows]);
  return <div className="space-y-sm p-lg">
    <Input type="search" aria-label="Find in output" placeholder="Find in output…" value={query} onChange={(event) => setQuery(event.target.value)} />
    <output aria-label="Output line count" aria-live="polite" className="block text-label text-text-subtle">
      {filtered.length === rows.length ? `${rows.length} lines` : `${filtered.length} of ${rows.length} lines`}
    </output>
    {filtered.length === 0 ? <p className="text-body-sm text-text-subtle">No lines match your search.</p> : <OutputList key={query} rows={filtered} />}
  </div>;
}

/** Row measurement updates stay inside the list while the search control remains stable. */
function OutputList({ rows }: { rows: readonly NumberedRow[] }) {
  const [width, setWidth] = useState(0);
  const list = useListRef(null);
  // Wrapped row measurements change the final offset after End; retain the row until new user input.
  const edge = useRef<"start" | "end" | null>(null);
  const release = () => {edge.current = null;};
  const alignEdge = useCallback(() => {
    if (edge.current !== null) list.current?.scrollToRow({index: edge.current === "start" ? 0 : rows.length - 1, align: edge.current, behavior: "instant"});
  }, [list, rows.length]);
  const navigate = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Home" && event.key !== "End") {release(); return;}
    event.preventDefault(); edge.current = event.key === "Home" ? "start" : "end"; alignEdge();
  };
  const height = useDynamicRowHeight({ defaultRowHeight: ESTIMATED_LINE_HEIGHT, key: width });
  const resized = useCallback((size: { width: number }) => setWidth(size.width), []);
  return <List aria-label="Command output lines" tabIndex={0} listRef={list}
    onKeyDown={navigate} onRowsRendered={alignEdge} onWheel={release} onPointerDown={release} onTouchStart={release}
    className="h-96 max-h-[50dvh] w-full font-mono text-mono leading-mono text-foreground"
    defaultHeight={DEFAULT_VIEWPORT_HEIGHT} rowCount={rows.length} rowHeight={height} onResize={resized}
    rowComponent={OutputRow} rowProps={{ rows }} />;
}

function OutputRow({ index, style, ariaAttributes, rows }: RowComponentProps<{ rows: readonly NumberedRow[] }>) {
  return <div {...ariaAttributes} style={style} className="grid grid-cols-[4ch_minmax(0,1fr)] gap-x-md">
    {rows.slice(index, index + 1).map((row) => <FleetOutputLine key={row.number} row={row} />)}
  </div>;
}

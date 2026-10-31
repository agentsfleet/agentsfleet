"use client";

import type { RowComponentProps } from "react-window";
import type { NumberedRow } from "./FleetOutputLine";
import { useCallback, useMemo, useState } from "react";
import { List, useDynamicRowHeight } from "react-window";
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
  const height = useDynamicRowHeight({ defaultRowHeight: ESTIMATED_LINE_HEIGHT, key: width });
  const resized = useCallback((size: { width: number }) => setWidth(size.width), []);
  return <List aria-label="Command output lines" tabIndex={0}
    className="h-96 max-h-[50dvh] w-full font-mono text-mono leading-mono text-foreground"
    defaultHeight={DEFAULT_VIEWPORT_HEIGHT} rowCount={rows.length} rowHeight={height} onResize={resized}
    rowComponent={OutputRow} rowProps={{ rows }} />;
}

function OutputRow({ index, style, ariaAttributes, rows }: RowComponentProps<{ rows: readonly NumberedRow[] }>) {
  return <div {...ariaAttributes} style={style} className="grid grid-cols-[4ch_minmax(0,1fr)] gap-x-md">
    {rows.slice(index, index + 1).map((row) => <FleetOutputLine key={row.number} row={row} />)}
  </div>;
}

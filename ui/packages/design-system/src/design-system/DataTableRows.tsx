import { useId, type MouseEvent, type ReactNode } from "react";
import type { ReactTable } from "@tanstack/react-table";
import { ChevronRight } from "lucide-react";
import { cn } from "../utils";
import { Button } from "./Button";
import type { DataTableFeatures } from "./DataTableModel";
import { DATA_TABLE_ROLE, DATA_TABLE_SURFACE, type DataTableColumn, type DataTableDisclosure, type DataTableRowData, type DataTableSurface } from "./DataTable.types";

const INTERACTIVE_CELL_SELECTOR = "a, button, input, select, textarea, [role='button'], [role='link'], [role='checkbox'], [role='combobox'], [contenteditable='true']";
type TableRow<T extends DataTableRowData> = ReturnType<ReactTable<DataTableFeatures, T>["getRowModel"]>["rows"][number];
type RowProps<T extends DataTableRowData> = {
  row: TableRow<T>;
  columnsByKey: Map<string, DataTableColumn<T>>;
  onRowClick?: (row: T) => void;
  rowDisclosure?: DataTableDisclosure<T>;
  surface: DataTableSurface;
  isLoading?: boolean;
};

export function DataTableRows<T extends DataTableRowData>({table, empty, ...props}: Omit<RowProps<T>, "row"> & {
  table: ReactTable<DataTableFeatures, T>;
  empty: ReactNode;
}) {
  const rows = table.getRowModel().rows;
  return <tbody role={DATA_TABLE_ROLE.rowgroup}>{rows.length === 0
    ? <tr role={DATA_TABLE_ROLE.row}><td role={DATA_TABLE_ROLE.cell} colSpan={props.columnsByKey.size + (props.onRowClick ? 1 : 0)}>{empty}</td></tr>
    : rows.map(row => <DataTableRow key={row.id} row={row} {...props} />)}</tbody>;
}

function activate(event: MouseEvent<HTMLTableRowElement>, action: (() => void) | undefined) {
  if (event.defaultPrevented || (event.target instanceof Element && event.target.closest(INTERACTIVE_CELL_SELECTOR))) return;
  action?.();
}

function DataTableRow<T extends DataTableRowData>({row, columnsByKey, onRowClick, rowDisclosure, surface, isLoading}: RowProps<T>) {
  const id = useId();
  const buttonId = `${id}-toggle`;
  const contentId = `${id}-detail`;
  const expanded = rowDisclosure?.expandedRow === row.id;
  const toggle = rowDisclosure && !isLoading
    ? () => rowDisclosure.onExpandedRowChange(expanded ? "" : row.id) : undefined;
  const open = onRowClick ? () => onRowClick(row.original) : toggle;
  return <>
    <tr className={cn(
      "group/row border-b border-table-divider transition-colors duration-snap ease-snap hover:bg-table-row-hover focus-within:bg-table-row-hover motion-reduce:transition-none",
      surface === DATA_TABLE_SURFACE.panel ? "bg-table-row even:bg-table-row-alternate last:border-b-0" : "bg-transparent",
      open && "cursor-pointer",
      onRowClick && "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
    )} role={DATA_TABLE_ROLE.row} onClick={event => activate(event, open)} tabIndex={onRowClick ? 0 : undefined}
      onKeyDown={onRowClick ? event => {
        if (event.defaultPrevented || event.target !== event.currentTarget) return;
        if (event.key === "Enter" || event.key === " ") {event.preventDefault(); onRowClick(row.original);}
      } : undefined}>
      <DataTableCells row={row} columnsByKey={columnsByKey} disclosure={rowDisclosure ? {
        buttonId, contentId, expanded, toggle, label: rowDisclosure.label(row.original), disabled: isLoading,
      } : undefined} />
      {onRowClick ? <td role={DATA_TABLE_ROLE.cell} className="w-10 pr-table-inline text-muted-foreground group-hover/row:text-pulse group-focus-within/row:text-pulse"><ChevronRight size={16} aria-hidden="true" /></td> : null}
    </tr>
    {rowDisclosure ? <tr role={DATA_TABLE_ROLE.row} hidden={!expanded} className="border-b border-table-divider">
      <td role={DATA_TABLE_ROLE.cell} colSpan={row.getAllCells().length + (onRowClick ? 1 : 0)}>
        <div id={contentId} role="region" aria-labelledby={buttonId} className={cn("p-lg", surface === DATA_TABLE_SURFACE.plain ? "bg-card" : "bg-muted/30")}>
          {expanded ? rowDisclosure.content(row.original) : null}
        </div>
      </td>
    </tr> : null}
  </>;
}

type ToggleProps = {buttonId: string; contentId: string; expanded: boolean; label: string; toggle?: () => void; disabled?: boolean};

function DataTableCells<T extends DataTableRowData>({row, columnsByKey, disclosure}: Pick<RowProps<T>, "row" | "columnsByKey"> & {disclosure?: ToggleProps}) {
  return row.getAllCells().map((cell, index) => {
    const definition = columnsByKey.get(cell.column.id)!;
    // Calling the renderer preserves cell state when its callback is recreated.
    const content = definition.cell(row.original);
    return <td key={cell.id} role={DATA_TABLE_ROLE.cell} className={cn(
      "h-12 px-table-inline py-table-block align-middle text-foreground",
      definition.primary && "font-medium", definition.numeric && "text-right tabular-nums",
      definition.numeric && cell.column.getCanSort() && "pr-3xl",
      definition.hideOnMobile && "hidden sm:table-cell",
    )}>{index === 0 && disclosure ? <div className="flex items-center gap-sm">
      <Button type="button" variant="ghost" size="icon-sm" className="text-muted-foreground"
        id={disclosure.buttonId} aria-controls={disclosure.contentId} aria-expanded={disclosure.expanded}
        aria-label={disclosure.label} disabled={disclosure.disabled} onClick={disclosure.toggle}>
        <ChevronRight className={cn("size-3.5 transition-transform motion-reduce:transition-none", disclosure.expanded && "rotate-90")} aria-hidden="true" />
      </Button><div className="min-w-0">{content}</div>
    </div> : content}</td>;
  });
}

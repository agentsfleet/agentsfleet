import type { DataTableColumn } from "./DataTable";

export type Row = { id: string; name: string; spend: number };

export const ROWS: Row[] = [
  { id: "a", name: "Alpha", spend: 12 },
  { id: "b", name: "Bravo", spend: 34 },
];

export const COLUMNS: DataTableColumn<Row>[] = [
  { key: "name", header: "Name", primary: true, cell: (r) => r.name, sortValue: (r) => r.name },
  { key: "spend", header: "Spend", numeric: true, cell: (r) => `$${r.spend}`, sortValue: (r) => r.spend },
];

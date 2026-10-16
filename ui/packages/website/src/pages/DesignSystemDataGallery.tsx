import { DataTable, Nav, NavItem, Section, SectionLabel, StatusPill, UsageBar } from "@agentsfleet/design-system";
import type { DataTableColumn } from "@agentsfleet/design-system";

const TABLE_STATUS = {
  active: {label: "Active", variant: "success"},
  waiting: {label: "Needs access", variant: "warning"},
  paused: {label: "Paused", variant: "neutral"},
} as const;
type FleetRow = {name: string; job: string; runs: number; status: typeof TABLE_STATUS[keyof typeof TABLE_STATUS]};
const ROWS: FleetRow[] = [
  {name: "Checkout watch", job: "Investigate failed payments", runs: 128, status: TABLE_STATUS.active},
  {name: "Release reviewer", job: "Review release changes", runs: 42, status: TABLE_STATUS.active},
  {name: "Support triage", job: "Route incoming requests", runs: 0, status: TABLE_STATUS.waiting},
  {name: "Dependency check", job: "Track dependency updates", runs: 16, status: TABLE_STATUS.paused},
  {name: "Incident follow-up", job: "Check completed repairs", runs: 23, status: TABLE_STATUS.active},
];
const PAGE_SIZE = 3;
const PAGE_SIZES = [PAGE_SIZE, ROWS.length];
const COLUMNS: DataTableColumn<FleetRow>[] = [
  {key: "name", header: "Fleet", cell: (row) => <span className="font-medium">{row.name}</span>, sortValue: (row) => row.name},
  {key: "job", header: "Job", cell: (row) => row.job, hideOnMobile: true},
  {key: "status", header: "Status", cell: (row) => <StatusPill variant={row.status.variant}>{row.status.label}</StatusPill>, sortValue: (row) => row.status.label},
  {key: "runs", header: "Runs", cell: (row) => row.runs, sortValue: (row) => row.runs, numeric: true},
];
const rowKey = (row: FleetRow): string => row.name;

export function AppPrimitivesGallery() {
  return <>
    <Section data-testid="app-primitives">
      <SectionLabel>App navigation and usage</SectionLabel>
      <Nav aria-label="Example app sections" className="flex flex-wrap gap-sm">
        <NavItem href="#app-fleets" active>Fleets</NavItem>
        <NavItem href="#app-events">Events</NavItem>
      </Nav>
      <UsageBar label="Monthly run budget" pct={62} sublabel="Budget remaining for this month" />
      <code className="font-mono text-mono">fleet_example</code>
    </Section>
    <Section id="data-table">
      <h2>Data table</h2>
      <p className="text-body-sm text-text-muted">Compare fleets, sort columns, and move between pages. Example data.</p>
      <DataTable columns={COLUMNS} rows={ROWS} rowKey={rowKey} caption="Example fleets"
        pagination={{pageSize: PAGE_SIZE, pageSizeOptions: PAGE_SIZES}} />
    </Section>
  </>;
}

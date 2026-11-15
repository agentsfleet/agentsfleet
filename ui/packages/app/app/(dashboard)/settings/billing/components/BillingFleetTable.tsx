"use client";

import type {BillingFleetBreakdown} from "@/lib/api/billing_report";
import {DataTable, List, ListItem, type DataTableColumn} from "@agentsfleet/design-system";
import {useModelLabels} from "@/components/domain/models/ModelLabels";
import {modelLabel, type ModelLabels} from "@/lib/model-labels";
import {formatBalance} from "@/lib/money";
import {formatCount} from "@/lib/events/run-figures-format";

/** Only the named table waits for labels; totals and daily bars need just the report. */
export function BillingFleetTable({rows}: {rows: BillingFleetBreakdown[]}) {
  const labels = useModelLabels();
  return <DataTable columns={fleetColumns(labels)} rows={rows} rowKey={row => row.key ?? "setup"}
    caption="Spending by fleet" pagination={false} />;
}

function fleetColumns(labels: ModelLabels): DataTableColumn<BillingFleetBreakdown>[] {
  return [
  {key: "fleet", header: "Fleet", primary: true, cell: row => <div><p className="text-label font-semibold">{row.label ?? row.key ?? "Setup chats"}</p><p className="mt-sm text-eyebrow leading-label text-text-dim">{row.key ? "Fleet work" : "Fleet setup"}</p></div>},
  {key: "models", header: "Model", cell: row => row.models.length === 0 ? "No model usage" : <List variant="plain" className="space-y-sm text-label break-words [overflow-wrap:anywhere]">{row.models.map(model => <ListItem key={model}>{modelLabel(labels, model)}</ListItem>)}</List>},
  {key: "activities", header: "Activities", cell: row => formatCount(row.totals.activity_count)},
  {key: "charged", header: "Charged", numeric: true, cell: row => formatBalance(row.totals.charged_nanos)},
  ];
}

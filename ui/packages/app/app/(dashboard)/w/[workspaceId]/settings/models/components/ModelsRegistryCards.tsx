import { Card, CardHeader, CardContent, CardFooter, type DataTableColumn } from "@agentsfleet/design-system";
import { CpuIcon } from "@phosphor-icons/react/ssr";
import { rowKey, type RegistryRow } from "./ModelsRegistryCells";

type Props = {rows: RegistryRow[]; columns: DataTableColumn<RegistryRow>[]};

/** Shared cards compose existing model readers and authorized action owners. */
export function ModelsRegistryCards({rows, columns}: Props) {
  const defaults = rows.filter(row => row.kind === "default");
  const configured = rows.filter(row => row.kind === "entry");
  return <div className="space-y-2xl">
    {defaults.map(row => <ModelCard key={rowKey(row)} row={row} columns={columns} />)}
    {configured.length > 0 ? <section aria-label="Your models" className="space-y-lg">
      <h2 className="flex items-center gap-md text-label font-medium">Your models<span className="rounded-sm border border-border px-sm py-xs text-eyebrow text-muted-foreground">{configured.length}</span></h2>
      {configured.map(row => <ModelCard key={rowKey(row)} row={row} columns={columns} />)}
    </section> : null}
  </div>;
}

function ModelCard({row, columns}: {row: RegistryRow; columns: DataTableColumn<RegistryRow>[]}) {
  const cells = Object.fromEntries(columns.map(column => [column.key, column.cell(row)]));
  const labels = Object.fromEntries(columns.map(column => [column.key, column.header]));
  return <Card aria-label={row.kind === "default" ? "Platform default" : row.entry.model_id} className="rounded-md p-xl sm:p-2xl">
    <div className="flex flex-wrap items-center gap-xl">
      <div aria-hidden="true" className="hidden size-10 shrink-0 items-center justify-center rounded-control border border-border text-muted-foreground sm:flex"><CpuIcon size={18} /></div>
      <div className="min-w-0 flex-1">
        <CardHeader className="pb-sm"><div className="min-w-0">{cells.model}</div></CardHeader>
        <CardContent className="flex flex-wrap items-center gap-lg"><div data-slot="model-provider">{cells.provider}</div>{cells.status}</CardContent>
      </div>
      <CardFooter className="min-w-0 max-w-full pt-0">{cells.actions}</CardFooter>
    </div>
    <CardContent className="mt-lg flex flex-wrap gap-x-2xl gap-y-sm text-eyebrow text-muted-foreground"><div className="flex flex-wrap gap-sm"><span>{labels.context}</span>{cells.context}</div><div className="flex flex-wrap gap-sm"><span>{labels.rates}</span>{cells.rates}</div></CardContent>
  </Card>;
}

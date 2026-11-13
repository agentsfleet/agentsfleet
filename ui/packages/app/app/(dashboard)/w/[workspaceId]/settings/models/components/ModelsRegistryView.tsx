"use client";

import type { ReactNode } from "react";
import { Badge, Button, DashboardPanel, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, Section, SectionHeader } from "@agentsfleet/design-system";
import { CpuIcon, DotsThreeIcon, KeyIcon } from "@phosphor-icons/react/ssr";
import { providerLabel } from "@/lib/api/model-library-types";
import type { TenantModelEntry, TenantPlatformDefault } from "@/lib/types";

type Props = {
  footerAction?: ReactNode;
  entries: TenantModelEntry[]; platformDefault: TenantPlatformDefault | null; workspaceNames: ReadonlyMap<string, string>;
  isDefaultLive: boolean; platformDefaultAvailable: boolean; pending: boolean;
  onSwitchDefault: () => void; onSwitchEntry: (entry: TenantModelEntry, workspaceId: string) => void;
  onView: (entry: TenantModelEntry) => void; onEdit: (entry: TenantModelEntry) => void; onRemove: (entry: TenantModelEntry) => void;
  onIntent: () => void; onLoadMore?: () => Promise<boolean>;
};

export function ModelsRegistryView(props: Props) {
  return <Section className="gap-3xl">
    <DashboardPanel className="flex flex-wrap items-center gap-xl p-xl sm:p-2xl" aria-label="Platform default">
      <div className="flex min-w-0 basis-full flex-col gap-xl sm:flex-1 sm:basis-auto sm:flex-row sm:items-center"><CpuIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
        <div className="min-w-0"><h2 className="text-label font-semibold">Platform default</h2>
        <p className="mt-sm text-description text-muted-foreground">{props.platformDefault ? `${props.platformDefault.display_name ?? props.platformDefault.model} · No provider key needed.` : "No platform default is configured."}</p></div></div>
      {props.isDefaultLive ? <Active /> : <Button variant="outline" aria-label="Make active: platform default" disabled={props.pending || !props.platformDefaultAvailable} onClick={props.onSwitchDefault}>Make active</Button>}
    </DashboardPanel>
    <section className="space-y-xl" aria-label="Your models">
      <SectionHeader className="h-5" actions={<span className="text-label text-muted-foreground">Provider keys stay hidden</span>}>Your models <Badge className="ml-md h-6.75">{props.entries.length}</Badge></SectionHeader>
      <DashboardPanel padding="none"><ul className="divide-y divide-border-subtle">
        {props.entries.length ? props.entries.map(entry => <ModelRow key={entry.id} entry={entry} {...props} />)
          : <li className="p-xl text-label text-muted-foreground">No models yet. Create a model to connect your own provider.</li>}
      </ul></DashboardPanel>
      {props.onLoadMore ? <Button variant="outline" disabled={props.pending} onClick={() => {void props.onLoadMore?.();}}>Load more models</Button> : null}
      <p className="text-eyebrow leading-eyebrow text-text-dim">Your active model is the default for new work. Each credential stays in its selected workspace.</p>
    </section>
    <div className="flex flex-wrap items-center justify-between gap-xl border-t border-border py-5"><div><p className="text-eyebrow text-text-dim">Bring your own provider</p>
      <p className="mt-sm text-description">Provider, model, and key. All in one form.</p></div>{props.footerAction}</div>
  </Section>;
}

function Active() {
  return <span className="inline-flex items-center gap-md text-label font-semibold text-success"><span className="size-1.5 rounded-full bg-success" aria-hidden="true" />Active</span>;
}

function ModelRow({entry, ...props}: Props & {entry: TenantModelEntry}) {
  const workspaceId = entry.workspace_id;
  return <li aria-label={entry.model_id} className="flex min-w-0 flex-wrap items-center gap-xl p-xl sm:p-5">
    <span className="hidden size-10 shrink-0 items-center justify-center rounded-control border border-border sm:flex" aria-hidden="true"><CpuIcon size={16} /></span>
    <div className="min-w-0 basis-full sm:flex-1 sm:basis-auto"><p title={entry.model_id} className="break-words text-label font-medium">{entry.display_name ?? entry.model_id}</p>
      <p className="mt-sm text-label text-muted-foreground">{entry.provider ? providerLabel(entry.provider) : "Unknown provider"}</p>
      <p className="mt-md flex items-center gap-md text-eyebrow text-text-dim"><KeyIcon size={14} aria-hidden="true" /><span className="break-words">{entry.has_key ? entry.secret_ref : "Key required"} · {props.workspaceNames.get(entry.workspace_id ?? "") ?? "Credential workspace"}</span></p></div>
    {entry.active ? <Active /> : <Button variant="outline" aria-label={`Make active: ${entry.model_id}`} title={!workspaceId ? "The credential workspace is unavailable" : !entry.has_key ? "A provider key is required" : undefined} disabled={props.pending || !workspaceId || !entry.has_key} onClick={workspaceId ? () => props.onSwitchEntry(entry, workspaceId) : undefined}>Make active</Button>}
    <DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="icon" disabled={props.pending} aria-label={`Manage ${entry.model_id}`} onFocus={props.onIntent} onPointerEnter={props.onIntent}><DotsThreeIcon size={16} /></Button></DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onSelect={() => props.onView(entry)}>Model details</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => props.onEdit(entry)}>Edit model</DropdownMenuItem>
        <DropdownMenuItem disabled={entry.active} onSelect={() => props.onRemove(entry)}>Remove model</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  </li>;
}

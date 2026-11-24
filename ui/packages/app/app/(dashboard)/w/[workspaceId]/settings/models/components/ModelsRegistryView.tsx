"use client";

import {
  Button, DashboardPanel, DropdownMenu, DropdownMenuContent, DropdownMenuItem,
  DropdownMenuTrigger, ResourceRow, Section, SectionHeader,
} from "@agentsfleet/design-system";
import { CpuIcon, DotsThreeIcon, KeyIcon } from "@phosphor-icons/react/ssr";
import { providerLabel } from "@/lib/api/model-library-types";
import type { TenantModelEntry, TenantPlatformDefault } from "@/lib/types";

type ModelActions = {
  pending: boolean;
  onSwitchEntry: (entry: TenantModelEntry, workspaceId: string) => void;
  onView: (entry: TenantModelEntry) => void;
  onEdit: (entry: TenantModelEntry) => void;
  onRemove: (entry: TenantModelEntry) => void;
  onIntent: () => void;
};

type Props = ModelActions & {
  entries: TenantModelEntry[];
  platformDefault: TenantPlatformDefault | null;
  workspaceNames: ReadonlyMap<string, string>;
  isDefaultLive: boolean;
  platformDefaultAvailable: boolean;
  onSwitchDefault: () => void;
  onLoadMore?: () => Promise<boolean>;
};

export function ModelsRegistryView({
  entries, workspaceNames, platformDefault, isDefaultLive, platformDefaultAvailable,
  onSwitchDefault, onLoadMore, ...actions
}: Props) {
  return (
    <Section className="gap-3xl">
      <DashboardPanel aria-label="Platform default" className="flex flex-wrap items-center gap-xl summary:p-2xl">
        <div className="flex min-w-0 basis-full flex-col gap-xl summary:flex-1 summary:basis-auto summary:flex-row summary:items-center">
          <span className="shrink-0 text-muted-foreground" aria-hidden="true">
            <CpuIcon size={16} className="inline-block align-middle" />
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="text-label font-semibold">Platform default</h2>
            <p className="mt-sm text-description text-muted-foreground">
              {platformDefault ? platformDefault.display_name ?? platformDefault.model : "No platform default is configured."}
            </p>
          </div>
        </div>
        {isDefaultLive ? <Active /> : (
          <Button variant="outline" aria-label="Make active: platform default"
            disabled={actions.pending || !platformDefaultAvailable} onClick={onSwitchDefault}>Make active</Button>
        )}
      </DashboardPanel>

      <Section className="block space-y-lg" aria-label="Your models">
        <SectionHeader count={entries.length} actions={<span className="text-label text-muted-foreground">Provider keys stay hidden</span>}>
          Your models
        </SectionHeader>
        <DashboardPanel padding="none">
          <ul className="divide-y divide-border-subtle">
            {entries.length ? entries.map(entry => (
              <ModelRow key={entry.id} entry={entry} actions={actions}
                workspaceName={workspaceNames.get(entry.workspace_id ?? "") ?? "Credential workspace"} />
            )) : <li className="p-xl text-label text-muted-foreground">No models yet. Create a model to connect your own provider.</li>}
          </ul>
        </DashboardPanel>
        {onLoadMore && <Button variant="outline" disabled={actions.pending} onClick={() => { void onLoadMore(); }}>Load more models</Button>}
        <p className="text-eyebrow leading-eyebrow text-text-dim">Your active model is the default for new work. Each credential stays in its selected workspace.</p>
      </Section>

    </Section>
  );
}

function Active() {
  return <span className="inline-flex items-center gap-md text-label font-title text-success">
    <span aria-hidden="true" className="size-1.5 rounded-full bg-success" />Active
  </span>;
}

function ModelRow({entry, workspaceName, actions}: {entry: TenantModelEntry; workspaceName: string; actions: ModelActions}) {
  const workspaceId = entry.workspace_id;
  const switchHint = !workspaceId ? "The credential workspace is unavailable" : !entry.has_key ? "A provider key is required" : undefined;
  return (
    <ResourceRow aria-label={entry.model_id} icon={<CpuIcon size={16} />} actions={<>
      {entry.active ? <Active /> : (
        <Button variant="outline" aria-label={`Make active: ${entry.model_id}`} title={switchHint}
          disabled={actions.pending || !workspaceId || !entry.has_key}
          onClick={workspaceId ? () => actions.onSwitchEntry(entry, workspaceId) : undefined}>Make active</Button>
      )}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon" disabled={actions.pending} aria-label={`Manage ${entry.model_id}`}
            onFocus={actions.onIntent} onPointerEnter={actions.onIntent}><DotsThreeIcon aria-hidden="true" /></Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onSelect={() => actions.onView(entry)}>Model details</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => actions.onEdit(entry)}>Edit model</DropdownMenuItem>
          <DropdownMenuItem disabled={entry.active} onSelect={() => actions.onRemove(entry)}>Remove model</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </>}>
        <p title={entry.model_id} className="break-words text-label font-medium">{entry.display_name ?? entry.model_id}</p>
        <p className="mt-sm text-label text-muted-foreground">{entry.provider ? providerLabel(entry.provider) : "Unknown provider"}</p>
        <p className="mt-md flex items-center gap-md text-eyebrow text-text-dim">
          <KeyIcon size={14} aria-hidden="true" />
          <span className="break-words">{entry.has_key ? entry.secret_ref : "Key required"} · {workspaceName}</span>
        </p>
    </ResourceRow>
  );
}

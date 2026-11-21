"use client";

import type { ReactNode } from "react";
import {
  Badge, Button, DashboardPanel, DropdownMenu, DropdownMenuContent, DropdownMenuItem,
  DropdownMenuTrigger, Section, SectionHeader,
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
  footerAction?: ReactNode;
};

export function ModelsRegistryView({
  entries, workspaceNames, platformDefault, isDefaultLive, platformDefaultAvailable,
  onSwitchDefault, onLoadMore, footerAction, ...actions
}: Props) {
  return (
    <Section className="gap-3xl">
      <DashboardPanel aria-label="Platform default" className="model-default summary:p-2xl">
        <div className="model-default-identity">
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

      <section className="space-y-xl" aria-label="Your models">
        <SectionHeader className="models-heading" actions={<span className="text-label text-muted-foreground">Provider keys stay hidden</span>}>
          Your models <Badge className="ml-md h-6.75">{entries.length}</Badge>
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
      </section>

      <div className="models-provider-footer">
        <div>
          <p className="text-eyebrow text-text-dim">Bring your own provider</p>
          <p className="my-sm text-description">Provider, model, and key. All in one form.</p>
        </div>
        {footerAction}
      </div>
    </Section>
  );
}

function Active() {
  return <span className="model-active"><span aria-hidden="true" />Active</span>;
}

function ModelRow({entry, workspaceName, actions}: {entry: TenantModelEntry; workspaceName: string; actions: ModelActions}) {
  const workspaceId = entry.workspace_id;
  const switchHint = !workspaceId ? "The credential workspace is unavailable" : !entry.has_key ? "A provider key is required" : undefined;
  return (
    <li aria-label={entry.model_id} className="settings-resource-row">
      <span className="settings-resource-icon" aria-hidden="true"><CpuIcon size={16} /></span>
      <div className="settings-resource-copy">
        <p title={entry.model_id} className="break-words text-label font-medium">{entry.display_name ?? entry.model_id}</p>
        <p className="mt-sm text-label text-muted-foreground">{entry.provider ? providerLabel(entry.provider) : "Unknown provider"}</p>
        <p className="model-credential">
          <KeyIcon size={14} aria-hidden="true" />
          <span className="break-words">{entry.has_key ? entry.secret_ref : "Key required"} · {workspaceName}</span>
        </p>
      </div>
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
    </li>
  );
}

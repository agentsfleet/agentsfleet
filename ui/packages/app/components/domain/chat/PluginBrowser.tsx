"use client";

import type { Source } from "@/lib/api/draft-types";
import type { Plugin, PluginCard } from "@/lib/api/draft-plugin";
import { CaretDownIcon, DotsThreeIcon, NotePencilIcon, PencilSimpleIcon, PuzzlePieceIcon } from "@phosphor-icons/react/ssr";
import { useCallback, useEffect, useState } from "react";
import { workspacePath } from "@/lib/workspace-routes";
import { Badge, Button, Collapsible, CollapsibleContent, CollapsibleTrigger, DashboardPanel, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, List, ListItem, Spinner, UnavailableState } from "@agentsfleet/design-system";
import { FleetPluginEmptyState } from "@/components/domain/FleetPluginEmptyState";
import { inspectPlugin, pluginHistory, pluginPage } from "@/lib/api/draft-plugin";
import { losesAccess } from "@/lib/api/draft-request";
import { PLUGIN_UNAVAILABLE_TITLE, pluginUnavailableDescription } from "@/lib/plugin-copy";
import { SOURCE_KIND } from "@/lib/api/draft-vocabulary";
import { useCurrentUser } from "@/lib/auth/client";
import { FleetSetupDetails } from "./FleetSetupDetails";
import { PluginRemoval } from "./PluginRemoval";

type Page = {readonly items: readonly PluginCard[]; readonly next_cursor: string | null};
const EMPTY: Page = {items: [], next_cursor: null};

function pluginErrorMessage(cause: unknown, selected: boolean): string {
  return pluginUnavailableDescription(losesAccess(cause) ? cause.status : undefined, selected);
}

type Props = {workspace: string; onSelect: (source: Extract<Source, {kind: typeof SOURCE_KIND.plugin}>, intent: "edit" | "new") => void};

export function PluginBrowser(props: Props) {
  const user = useCurrentUser();
  if (!user.isLoaded) return <Spinner label="Loading account" />;
  if (!user.isSignedIn || !user.userId) return null;
  return <WorkspacePlugins key={`${user.userId}:${props.workspace}`} {...props} />;
}

function WorkspacePlugins({workspace, onSelect}: Props) {
  const [cursor, setCursor] = useState<string | null>(null);
  const [page, setPage] = useState<Page | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [inspectionAttempt, setInspectionAttempt] = useState(0);
  const [selected, setSelected] = useState<PluginCard | null>(null);
  const [revision, setRevision] = useState<string | undefined>();
  const [entry, setEntry] = useState<Plugin | null>(null);
  const [error, setError] = useState<string | null>(null);
  const failed = useCallback((cause: unknown, selected = false): void => {
    if (losesAccess(cause)) { setSelected(null); setEntry(null); setPage(EMPTY); setRevision(undefined); }
    setError(pluginErrorMessage(cause, selected));
  }, []);
  const selectionFailed = useCallback((cause: unknown) => failed(cause, true), [failed]);
  useEffect(() => {
    const controller = new AbortController();
    setError(null); setEntry(null); setPage(null);
    void pluginPage(workspace, cursor, controller.signal).then((result) => {
      if (!controller.signal.aborted) setPage(result);
    }).catch((cause: unknown) => {
      if (!controller.signal.aborted) failed(cause);
    });
    return () => controller.abort();
  }, [workspace, cursor, attempt, failed]);
  useEffect(() => {
    const controller = new AbortController();
    setEntry(null);
    if (selected) void inspectPlugin(workspace, selected, controller.signal, revision).then((result) => {
      if (!controller.signal.aborted) setEntry(result);
    }).catch((cause: unknown) => {
      if (!controller.signal.aborted) selectionFailed(cause);
    });
    return () => controller.abort();
  }, [workspace, selected, revision, attempt, inspectionAttempt, selectionFailed]);
  return <div className="space-y-lg">
    <div className="flex items-center justify-between gap-xl"><h2 className="text-label font-semibold">Your plugins {page && !cursor && !page.next_cursor ? <Badge className="ml-md">{page.items.length}</Badge> : null}</h2><span className="text-label text-muted-foreground">Current account</span></div>
    {error && !selected ? <UnavailableState title={PLUGIN_UNAVAILABLE_TITLE} description={error}
      actions={<Button variant="outline" onClick={() => setAttempt((value) => value + 1)}>Retry plugins</Button>} /> : null}
    {!page && !error ? <Spinner label="Loading published setups" /> : null}
    <PluginChoices page={page}
      onInspect={(card) => { setEntry(null); setSelected(card); setRevision(undefined); setError(null); setInspectionAttempt((value) => value + 1); }} />
    {!error && page && !page.items.length ? <PluginEmpty workspace={workspace} paged={!!(cursor || page.next_cursor)} /> : null}
    {cursor ? <Button variant="outline" onClick={() => { setCursor(null); setSelected(null); }}>First page</Button> : null}
    {page?.next_cursor ? <Button variant="outline" onClick={() => { setCursor(page.next_cursor); setSelected(null); }}>Next page</Button> : null}
    <p className="text-eyebrow leading-eyebrow text-text-dim">Each new fleet keeps its own setup and history. Changing or deleting a Plugin does not change fleets already created from it.</p>
    <Dialog open={selected !== null} onOpenChange={() => {setSelected(null); setEntry(null);}}><DialogContent>
      <DialogHeader><DialogTitle>{selected?.name ?? "Plugin"}</DialogTitle><DialogDescription>{selected?.description}</DialogDescription></DialogHeader>
    {error && selected ? <UnavailableState title="This plugin could not be loaded" description={error}
      actions={<Button variant="outline" onClick={() => {setError(null); setInspectionAttempt(value => value + 1);}}>Retry plugin</Button>} /> : null}
    {selected ? <Collapsible><CollapsibleTrigger asChild><Button variant="ghost" className="px-0">Choose a saved version<CaretDownIcon size={16} aria-hidden="true" /></Button></CollapsibleTrigger><CollapsibleContent><PluginHistory key={selected.id} workspace={workspace} plugin={selected.id} onError={selectionFailed} onSelect={(id) => { setEntry(null); setRevision(id); setError(null); setInspectionAttempt((value) => value + 1); }} /></CollapsibleContent></Collapsible> : null}
    {selected && !entry && !error ? <Spinner label="Loading selected revision" /> : null}
    {entry && selected && !error ? <PluginInspection entry={entry} onUse={intent => { onSelect({kind: SOURCE_KIND.plugin, plugin_id: entry.card.id, revision_id: entry.head.id}, intent); }} /> : null}
    {selected ? <div className="pt-md"><PluginRemoval workspace={workspace} card={selected} onRemoved={() => {setSelected(null); setCursor(null); setAttempt(value => value + 1);}} /></div> : null}
    </DialogContent></Dialog>
  </div>;
}

function PluginEmpty({workspace, paged}: {workspace: string; paged: boolean}) {
  const copy = paged ? "No published setups on this page. Check another page."
    : "No published setups are available yet. Start a fleet setup chat, then save or publish your setup.";
  return <FleetPluginEmptyState createHref={workspacePath(workspace, "fleets/new")} description={copy} />;
}

function PluginChoices({page, onInspect}: {page: Page | null; onInspect: (card: PluginCard) => void}) {
  return <DashboardPanel padding="none"><List variant="plain" className="space-y-0 divide-y divide-border">{page?.items.map(card => <ListItem key={card.id} className="flex flex-wrap items-center gap-xl p-xl sm:p-5">
    <span aria-hidden="true" className="hidden size-10 shrink-0 items-center justify-center rounded-control border border-border sm:flex"><PuzzlePieceIcon size={16} /></span>
    <div className="min-w-0 basis-full sm:flex-1 sm:basis-auto"><p className="break-words text-label font-medium">{card.name}</p><p className="mt-sm text-label text-muted-foreground">{card.description}</p></div>
    <span className="text-eyebrow text-text-dim">Published setup</span>
    <Button variant="ghost" size="icon" aria-label={`Manage ${card.name}`} onClick={() => onInspect(card)}><DotsThreeIcon size={16} /></Button>
  </ListItem>)}</List></DashboardPanel>;
}

function PluginHistory({workspace, plugin, onSelect, onError}: {workspace: string; plugin: string; onSelect: (id: string) => void; onError: (cause: unknown) => void}) {
  const [cursor, setCursor] = useState<string | null>(null);
  const [page, setPage] = useState<Awaited<ReturnType<typeof pluginHistory>> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    setPage(null); setError(null);
    void pluginHistory(workspace, plugin, cursor, controller.signal).then((result) => {
      if (!controller.signal.aborted) setPage(result);
    }).catch((cause: unknown) => {
      if (!controller.signal.aborted) {
        if (losesAccess(cause)) onError(cause);
        else setError(pluginErrorMessage(cause, true));
      }
    });
    return () => controller.abort();
  }, [workspace, plugin, cursor, attempt, onError]);
  return <div className="space-y-sm" aria-label="Published revisions">
    <h3 className="text-sm font-medium">Published revisions</h3>
    {!page && !error ? <Spinner label="Loading published revisions" /> : null}
    {error ? <UnavailableState title="Revision history is unavailable right now" description={error}
      actions={<Button variant="outline" onClick={() => setAttempt((value) => value + 1)}>Retry revision history</Button>} /> : null}
    <div className="flex flex-wrap gap-sm">{page?.items.map((revision) => <Button variant="outline" key={revision.id} onClick={() => onSelect(revision.id)}>Inspect revision {revision.revision}</Button>)}</div>
    {page?.next_cursor ? <Button variant="ghost" onClick={() => setCursor(page.next_cursor)}>Older revisions</Button> : null}
  </div>;
}

function PluginInspection({entry, onUse}: {entry: Plugin; onUse: (intent: "edit" | "new") => void}) {
  return <div className="mt-md space-y-md">
    <p className="text-eyebrow text-text-dim">Version {entry.head.revision}</p>
    <Button variant="outline" className="min-h-11 w-full justify-start gap-md" onClick={() => onUse("edit")}><PencilSimpleIcon size={16} /><span>Edit setup</span></Button>
    <Button variant="outline" className="min-h-11 w-full justify-start gap-md" onClick={() => onUse("new")}><NotePencilIcon size={16} /><span>Use for new fleet</span></Button>
    <Collapsible><CollapsibleTrigger asChild><Button variant="ghost" className="px-0">Inspect setup<CaretDownIcon size={16} aria-hidden="true" /></Button></CollapsibleTrigger>
      <CollapsibleContent className="pt-lg"><FleetSetupDetails review={entry.review} /></CollapsibleContent></Collapsible>
  </div>;
}

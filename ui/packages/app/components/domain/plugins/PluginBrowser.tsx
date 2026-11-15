"use client";

import type { Source } from "@/lib/api/draft-types";
import type { PluginCard } from "@/lib/api/draft-plugin";
import { DotsThreeIcon, PuzzlePieceIcon } from "@phosphor-icons/react/ssr";
import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import nextDynamic from "next/dynamic";
import { workspacePath } from "@/lib/workspace-routes";
import { Badge, Button, DashboardPanel, List, ListItem, Spinner, UnavailableState } from "@agentsfleet/design-system";
import { PluginEmptyState } from "@/components/domain/plugins/PluginEmptyState";
import { pluginPage } from "@/lib/api/draft-plugin";
import { losesAccess } from "@/lib/api/draft-request";
import { PLUGIN_UNAVAILABLE_TITLE, pluginUnavailableDescription } from "@/lib/plugin-copy";
import { SOURCE_KIND } from "@/lib/api/draft-vocabulary";
import { useCurrentUser } from "@/lib/auth/client";

const PluginInspector = nextDynamic(() => import("./PluginInspector").then(module => module.PluginInspector));
type Page = {readonly items: readonly PluginCard[]; readonly next_cursor: string | null};
const EMPTY: Page = {items: [], next_cursor: null};
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
  const [selected, setSelected] = useState<PluginCard | null>(null);
  const [error, setError] = useState<string | null>(null);
  const failed = useCallback((cause: unknown): void => {
    if (losesAccess(cause)) {setSelected(null);setPage(EMPTY);}
    setError(pluginUnavailableDescription(losesAccess(cause) ? cause.status : undefined, false));
  }, []);
  useEffect(() => {
    const controller = new AbortController();setError(null);setPage(null);
    void pluginPage(workspace, cursor, controller.signal).then(result => {
      if (!controller.signal.aborted) setPage(result);
    }).catch(cause => {if (!controller.signal.aborted) failed(cause);});
    return () => controller.abort();
  }, [workspace, cursor, attempt, failed]);
  const createHref = `${workspacePath(workspace, "fleets/new")}?starter=plugin`;
  return <div className="space-y-lg">
    {page?.items.length || cursor || page?.next_cursor ? <div className="flex flex-wrap items-center justify-between gap-xl"><div><h2 className="flex min-h-7 items-center gap-md text-label font-semibold">Your plugins {page && !cursor && !page.next_cursor ? <Badge className="flex h-5 min-w-5 items-center justify-center px-sm tabular-nums">{page.items.length}</Badge> : null}</h2><p className="mt-xs text-label text-muted-foreground">Current account</p></div><Button asChild><Link href={createHref}>Create a plugin</Link></Button></div> : null}
    {error ? <UnavailableState title={PLUGIN_UNAVAILABLE_TITLE} description={error} actions={<Button variant="outline" onClick={() => setAttempt(value => value + 1)}>Retry plugins</Button>} /> : null}
    {!page && !error ? <Spinner label="Loading published setups" /> : null}
    {page?.items.length ? <DashboardPanel padding="none"><List variant="plain" className="space-y-0 divide-y divide-border">{page.items.map(card => <ListItem key={card.id} className="flex flex-wrap items-center gap-xl p-xl sm:p-5">
      <span aria-hidden="true" className="hidden size-10 shrink-0 items-center justify-center rounded-control border border-border sm:flex"><PuzzlePieceIcon size={16} /></span>
      <div className="min-w-0 basis-full sm:flex-1 sm:basis-auto"><p className="break-words text-label font-medium">{card.name}</p><p className="mt-sm text-label text-muted-foreground">{card.description}</p></div>
      <span className="text-eyebrow text-text-dim">Published setup</span>
      <Button variant="ghost" size="icon" aria-label={`Manage ${card.name}`} onClick={() => setSelected(card)}><DotsThreeIcon size={16} /></Button>
    </ListItem>)}</List></DashboardPanel> : null}
    {!error && page && !page.items.length ? <PluginEmptyState createHref={createHref} paged={!!(cursor || page.next_cursor)}
      description={cursor || page.next_cursor ? "No published setups on this page. Check another page." : "Create a fleet setup in chat, then choose Save and publish to add its reusable setup here."} /> : null}
    {cursor ? <Button variant="outline" onClick={() => {setCursor(null);setSelected(null);}}>First page</Button> : null}
    {page?.next_cursor ? <Button variant="outline" onClick={() => {setCursor(page.next_cursor);setSelected(null);}}>Next page</Button> : null}
    {page?.items.length ? <p className="text-eyebrow leading-eyebrow text-text-dim">Each new fleet keeps its own setup and history. Changing or deleting a Plugin does not change fleets already created from it.</p> : null}
    {selected ? <PluginInspector key={selected.id} workspace={workspace} card={selected} onClose={() => setSelected(null)} onAccessLost={failed}
      onRemoved={() => {setSelected(null);setCursor(null);setAttempt(value => value + 1);}}
      onUse={(entry, intent) => onSelect({kind: SOURCE_KIND.plugin, plugin_id: entry.card.id, revision_id: entry.head.id}, intent)} /> : null}
  </div>;
}

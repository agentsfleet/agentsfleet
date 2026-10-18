"use client";

import type { Source } from "@/lib/api/draft-types";
import type { Library, LibraryCard } from "@/lib/api/draft-library";
import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { LibraryIcon, PlusIcon } from "lucide-react";
import { workspacePath } from "@/lib/workspace-routes";
import { Button, EmptyState, List, ListItem, Spinner, Tabs, TabsList, TabsTrigger, UnavailableState } from "@agentsfleet/design-system";
import { inspectLibrary, libraryHistory, libraryPage } from "@/lib/api/draft-library";
import { losesAccess } from "@/lib/api/draft-request";
import { LIBRARY_UNAVAILABLE_TITLE, libraryUnavailableDescription } from "@/lib/library-copy";
import { SOURCE_KIND } from "@/lib/api/draft-types";
import { FleetSetupDetails } from "./FleetSetupDetails";
import { LibraryRemoval } from "./LibraryRemoval";

type Page = {readonly items: readonly LibraryCard[]; readonly next_cursor: string | null};
const EMPTY: Page = {items: [], next_cursor: null};

function libraryErrorMessage(cause: unknown, selected: boolean): string {
  return libraryUnavailableDescription(losesAccess(cause) ? cause.status : undefined, selected);
}

export function LibraryBrowser({workspace, onSelect}: {workspace: string; onSelect: (source: Extract<Source, {kind: typeof SOURCE_KIND.library}>) => void}) {
  const [owned, setOwned] = useState(false);
  const [cursor, setCursor] = useState<string | null>(null);
  const [page, setPage] = useState<Page | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [inspectionAttempt, setInspectionAttempt] = useState(0);
  const [selected, setSelected] = useState<LibraryCard | null>(null);
  const [revision, setRevision] = useState<string | undefined>();
  const [entry, setEntry] = useState<Library | null>(null);
  const [error, setError] = useState<string | null>(null);
  const failed = useCallback((cause: unknown, selected = false): void => {
    if (losesAccess(cause)) { setSelected(null); setEntry(null); setPage(EMPTY); setRevision(undefined); }
    setError(libraryErrorMessage(cause, selected));
  }, []);
  const selectionFailed = useCallback((cause: unknown) => failed(cause, true), [failed]);
  useEffect(() => {
    const controller = new AbortController();
    setError(null); setEntry(null); setPage(null);
    void libraryPage(workspace, owned, cursor, controller.signal).then((result) => {
      if (!controller.signal.aborted) setPage(result);
    }).catch((cause: unknown) => {
      if (!controller.signal.aborted) failed(cause);
    });
    return () => controller.abort();
  }, [workspace, owned, cursor, attempt, failed]);
  useEffect(() => {
    const controller = new AbortController();
    setEntry(null);
    if (selected) void inspectLibrary(workspace, selected, controller.signal, revision).then((result) => {
      if (!controller.signal.aborted) setEntry(result);
    }).catch((cause: unknown) => {
      if (!controller.signal.aborted) selectionFailed(cause);
    });
    return () => controller.abort();
  }, [workspace, selected, revision, attempt, inspectionAttempt, selectionFailed]);
  return <div className="space-y-lg">
    <LibraryScope owned={owned} onChange={(value) => { setOwned(value); setCursor(null); setSelected(null); }} />
    {error ? <UnavailableState title={LIBRARY_UNAVAILABLE_TITLE} description={error}
      actions={<Button variant="outline" onClick={() => setAttempt((value) => value + 1)}>Retry Library</Button>} /> : null}
    {!page && !error ? <Spinner label="Loading published setups" /> : null}
    <LibraryChoices page={page} workspace={workspace} owned={owned} onRemoved={() => { setSelected(null); setCursor(null); setAttempt((value) => value + 1); }}
      onInspect={(card) => { setEntry(null); setSelected(card); setRevision(undefined); setError(null); setInspectionAttempt((value) => value + 1); }} />
    {!error && page && !page.items.length ? <LibraryEmpty workspace={workspace} paged={!!(cursor || page.next_cursor)} owned={owned} /> : null}
    {cursor ? <Button variant="outline" onClick={() => { setCursor(null); setSelected(null); }}>First page</Button> : null}
    {page?.next_cursor ? <Button variant="outline" onClick={() => { setCursor(page.next_cursor); setSelected(null); }}>Next page</Button> : null}
    {selected ? <LibraryHistory key={selected.id} workspace={workspace} library={selected.id} onError={selectionFailed} onSelect={(id) => { setEntry(null); setRevision(id); setError(null); setInspectionAttempt((value) => value + 1); }} /> : null}
    {selected && !entry && !error ? <Spinner label="Loading selected revision" /> : null}
    {entry ? <LibraryInspection entry={entry} onUse={() => { onSelect({kind: SOURCE_KIND.library, library_id: entry.card.id, revision_id: entry.head.id}); }} /> : null}
  </div>;
}

function LibraryScope({owned, onChange}: {owned: boolean; onChange: (owned: boolean) => void}) {
  const scope = {workspace: "workspace", all: "all"} as const;
  return <div className="space-y-md"><Tabs value={owned ? scope.workspace : scope.all} onValueChange={(value) => onChange(value === scope.workspace)}>
    <TabsList aria-label="Fleet library scope"><TabsTrigger value={scope.all}>All</TabsTrigger><TabsTrigger value={scope.workspace}>Workspace</TabsTrigger></TabsList>
  </Tabs><p className="text-sm text-muted-foreground">{owned
    ? "Setups published by your account, available in this workspace."
    : "Platform setups and setups published by your account. Private setups from other accounts are never included."}</p></div>;
}

function LibraryEmpty({workspace, paged, owned}: {workspace: string; paged: boolean; owned: boolean}) {
  const copy = paged ? "No published setups on this page. Check another page or change the library scope."
    : owned ? "No setups have been published by this workspace. Start a fleet setup chat, then save or publish your setup."
    : "No published setups are available yet. Start a fleet setup chat, then save or publish your setup.";
  return <EmptyState icon={<LibraryIcon size={28} />} title="Build your next fleet" description={copy}
    action={<Button asChild variant="secondary"><Link href={workspacePath(workspace, "fleets/new")}><PlusIcon size={16} aria-hidden="true" />New fleet</Link></Button>} />;
}

function LibraryChoices({page, workspace, owned, onRemoved, onInspect}: {page: Page | null; workspace: string; owned: boolean; onRemoved: () => void; onInspect: (card: LibraryCard) => void}) {
  return <List variant="plain" divided>{page?.items.map((card) => <ListItem key={card.id}>
    <div className="flex items-start justify-between gap-md"><div><p className="font-medium">{card.name}</p><p className="text-sm text-muted-foreground">{card.description}</p></div>
      <div className="flex items-center gap-sm"><Button variant="outline" onClick={() => onInspect(card)}>Inspect</Button>
        {owned && card.visibility === "tenant" ? <LibraryRemoval workspace={workspace} card={card} onRemoved={onRemoved} /> : null}</div></div>
  </ListItem>)}</List>;
}

function LibraryHistory({workspace, library, onSelect, onError}: {workspace: string; library: string; onSelect: (id: string) => void; onError: (cause: unknown) => void}) {
  const [cursor, setCursor] = useState<string | null>(null);
  const [page, setPage] = useState<Awaited<ReturnType<typeof libraryHistory>> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    setPage(null); setError(null);
    void libraryHistory(workspace, library, cursor, controller.signal).then((result) => {
      if (!controller.signal.aborted) setPage(result);
    }).catch((cause: unknown) => {
      if (!controller.signal.aborted) {
        if (losesAccess(cause)) onError(cause);
        else setError(libraryErrorMessage(cause, true));
      }
    });
    return () => controller.abort();
  }, [workspace, library, cursor, attempt, onError]);
  return <div className="space-y-sm" aria-label="Published revisions">
    <h3 className="text-sm font-medium">Published revisions</h3>
    {!page && !error ? <Spinner label="Loading published revisions" /> : null}
    {error ? <UnavailableState title="Revision history is unavailable right now" description={error}
      actions={<Button variant="outline" onClick={() => setAttempt((value) => value + 1)}>Retry revision history</Button>} /> : null}
    <div className="flex flex-wrap gap-sm">{page?.items.map((revision) => <Button variant="outline" key={revision.id} onClick={() => onSelect(revision.id)}>Inspect revision {revision.revision}</Button>)}</div>
    {page?.next_cursor ? <Button variant="ghost" onClick={() => setCursor(page.next_cursor)}>Older revisions</Button> : null}
  </div>;
}

function LibraryInspection({entry, onUse}: {entry: Library; onUse: () => void}) {
  return <div className="space-y-md border-t border-border pt-lg">
    <h3 className="font-medium">{entry.card.name} · revision {entry.head.revision}</h3>
    <FleetSetupDetails review={entry.review} />
    <Button onClick={onUse}>Start chat from this revision</Button>
  </div>;
}

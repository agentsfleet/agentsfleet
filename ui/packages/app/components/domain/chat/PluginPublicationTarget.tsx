"use client";

import {useEffect, useId, useState} from "react";
import {Button, Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue, Spinner} from "@agentsfleet/design-system";
import {libraryPage, type LibraryCard} from "@/lib/api/draft-library";

/** Selecting a destination is explicit intent, independent of a draft's source. */
export function PluginPublicationTarget({workspace, disabled, value, onChange}: {
  workspace: string; disabled: boolean; value: string | undefined; onChange: (id: string | undefined) => void;
}) {
  const id = useId();
  const [cursor, setCursor] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [page, setPage] = useState<{items: LibraryCard[]; next_cursor: string | null} | null>(null);
  const [error, setError] = useState(false);
  useEffect(() => {
    const controller = new AbortController();
    setPage(null); setError(false);
    void libraryPage(workspace, true, cursor, controller.signal).then(result => {
      if (!controller.signal.aborted) setPage({...result, items: result.items.filter(item => item.visibility === "tenant")});
    }).catch(() => {if (!controller.signal.aborted) setError(true);});
    return () => controller.abort();
  }, [workspace, cursor, attempt]);
  const move = (next: string | null) => {onChange(undefined); setPage(null); setCursor(next);};
  return <div className="space-y-md">
    <Label htmlFor={id}>Plugin to update</Label>
    {error ? <div role="alert"><p>Plugins could not be loaded.</p><Button variant="link" disabled={disabled} onClick={() => setAttempt(value => value + 1)}>Try again</Button></div>
      : !page ? <Spinner label="Loading your plugins" /> : <>
        {page.items.length ? <Select value={value ?? ""} onValueChange={onChange} disabled={disabled}>
          <SelectTrigger id={id}><SelectValue placeholder="Choose an existing plugin" /></SelectTrigger>
          <SelectContent>{page.items.map(item => <SelectItem key={item.id} value={item.id}>{item.name}</SelectItem>)}</SelectContent>
        </Select> : <p className="text-label text-muted-foreground">No plugins on this page.</p>}
        <div className="flex gap-md">{cursor ? <Button variant="outline" disabled={disabled} onClick={() => move(null)}>First page</Button> : null}
          {page.next_cursor ? <Button variant="outline" disabled={disabled} onClick={() => move(page.next_cursor)}>Next page</Button> : null}</div>
      </>}
    <p className="text-eyebrow text-text-dim">Publishing adds a version to the selected plugin. Existing fleets keep their setup.</p>
  </div>;
}

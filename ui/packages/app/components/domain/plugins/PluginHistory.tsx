"use client";

import { useEffect, useState } from "react";
import { Button, Spinner, UnavailableState } from "@agentsfleet/design-system";
import { pluginHistory } from "@/lib/api/draft-plugin";
import { losesAccess } from "@/lib/api/draft-request";
import { pluginUnavailableDescription } from "@/lib/plugin-copy";

export function PluginHistory({workspace, plugin, onSelect, onError}: {workspace: string; plugin: string; onSelect: (id: string) => void; onError: (cause: unknown) => void}) {
  const [cursor, setCursor] = useState<string | null>(null);
  const [page, setPage] = useState<Awaited<ReturnType<typeof pluginHistory>> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const controller = new AbortController();setPage(null);setError(null);
    void pluginHistory(workspace, plugin, cursor, controller.signal).then(result => {
      if (!controller.signal.aborted) setPage(result);
    }).catch(cause => {
      if (controller.signal.aborted) return;
      if (losesAccess(cause)) onError(cause);
      else setError(pluginUnavailableDescription(undefined, true));
    });
    return () => controller.abort();
  }, [workspace, plugin, cursor, attempt, onError]);
  return <div className="space-y-sm" aria-label="Published revisions">
    <h3 className="text-sm font-medium">Published revisions</h3>
    {!page && !error ? <Spinner label="Loading published revisions" /> : null}
    {error ? <UnavailableState title="Revision history is unavailable right now" description={error}
      actions={<Button variant="outline" onClick={() => setAttempt(value => value + 1)}>Retry revision history</Button>} /> : null}
    <div className="flex flex-wrap gap-sm">{page?.items.map(revision => <Button variant="outline" key={revision.id} onClick={() => onSelect(revision.id)}>Inspect revision {revision.revision}</Button>)}</div>
    {page?.next_cursor ? <Button variant="ghost" onClick={() => setCursor(page.next_cursor)}>Older revisions</Button> : null}
  </div>;
}

"use client";

import { Button, EmptyState, Grid, Section } from "@agentsfleet/design-system";
import { useDeferredValue, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { CollectionToolbar } from "@/components/domain/CollectionToolbar";
import { useWorkspaceNotice, WorkspaceNotice } from "@/components/layout/WorkspaceNotice";
import { PlugIcon } from "lucide-react";
import { type ConnectorCatalogEntry } from "@/lib/api/connectors";
import { OAuthConnectorRow } from "./connector-rows";
import { CONNECT_APPS_COPY } from "../copy";

const CONNECTOR_REQUESTED_MESSAGE = "Requested";

// The diagnosable shape of a failed catalog fetch, captured server-side in
// page.tsx (console logging is lint-banned in app source, so the code/status is
// surfaced here instead — the RFC 7807 error_code is the public, documented id).
// `status` is null when the failure wasn't an ApiError (no HTTP status to show).
export type ConnectorFetchError = { code: string; status: number | null };

// Empty catalog always means the fetch failed (the list is registry-driven and
// never legitimately empty), so name the cause when we have it. A null status
// renders as the code alone — never a fabricated "· 0".
function connectorsLoadFailedDescription(err: ConnectorFetchError | null | undefined): string {
  const detail = err ? ` (${err.code}${err.status === null ? "" : ` · ${err.status}`})` : "";
  return `Something went wrong fetching the connector list${detail} — refresh to try again.`;
}

export default function IntegrationsConnectors({
  workspaceId,
  catalog,
  catalogError = null,
}: {
  workspaceId: string;
  catalog: readonly ConnectorCatalogEntry[];
  catalogError?: ConnectorFetchError | null;
}) {
  const [search, setSearch] = useState("");
  const { notice, showNotice } = useWorkspaceNotice();
  const query = useDeferredValue(search.trim().toLowerCase());
  const [refreshing, startRefresh] = useTransition();
  const router = useRouter();
  const matches = catalog.filter((entry) => entry.display_name.toLowerCase().includes(query));

  return (
    <Section className="space-y-xl" data-testid="integrations-connectors">
      {catalog.length > 0 ? <CollectionToolbar label={CONNECT_APPS_COPY.search} value={search} onChange={setSearch} summary={`${matches.length} of ${catalog.length} apps`} /> : null}
      {catalog.length === 0 ? (
        // The catalog is registry-driven (Slack/GitHub/Zoho/Jira/Linear/…),
        // never legitimately empty — an empty list here always means the
        // fetch failed and degraded via the page's catch-all. Say so
        // plainly instead of the ambiguous "no connectors" framing.
        <EmptyState
          data-testid="connectors-empty"
          icon={<PlugIcon size={32} />}
          title="Couldn't load connectors"
          description={connectorsLoadFailedDescription(catalogError)}
          action={<Button variant="secondary" disabled={refreshing} onClick={() => startRefresh(() => router.refresh())}>{refreshing ? "Retrying…" : "Retry"}</Button>}
        />
      ) : (
        <Grid columns="three" className="grid-cols-1 sm:grid-cols-2 xl:grid-cols-3">
          {matches.map((entry) => (
            <OAuthConnectorRow
              key={entry.id}
              entry={entry}
              workspaceId={workspaceId}
              onRequest={() => showNotice("success", CONNECTOR_REQUESTED_MESSAGE)}
            />
          ))}
        </Grid>
      )}
      {catalog.length > 0 && matches.length === 0 ? <EmptyState title={CONNECT_APPS_COPY.noMatches} description="Try another service name or clear your search." /> : null}
      <WorkspaceNotice notice={notice} />
    </Section>
  );
}

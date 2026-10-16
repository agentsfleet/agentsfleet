"use client";

import { Button, EmptyState, Grid, Section } from "@agentsfleet/design-system";
import { useDeferredValue, useState, useTransition } from "react";
import { useRouter } from "next/navigation";
import { CollectionToolbar } from "@/components/domain/CollectionToolbar";
import { PlugIcon } from "lucide-react";
import { type ConnectorCatalogEntry } from "@/lib/api/connectors";
import { CONNECTOR_STATUS, type ConnectorStatus } from "@/lib/api/connectors-types";
import { OAuthConnectorRow, type ConnectorStatusOverride } from "./connector-rows";

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
  githubStatus,
  slackStatus = CONNECTOR_STATUS.notConnected,
  slackTeam = null,
}: {
  workspaceId: string;
  catalog: readonly ConnectorCatalogEntry[];
  catalogError?: ConnectorFetchError | null;
  githubStatus: ConnectorStatus;
  slackStatus?: ConnectorStatus;
  slackTeam?: string | null;
}) {
  const [search, setSearch] = useState("");
  const query = useDeferredValue(search.trim().toLowerCase());
  const [refreshing, startRefresh] = useTransition();
  const router = useRouter();
  const matches = catalog.filter((entry) => entry.display_name.toLowerCase().includes(query));
  // GitHub and Slack are the two connectors the page fetches a bespoke status for
  // (tri-state + the Slack team); every other card derives its status from the
  // catalog. Keyed by provider id so the render loop stays provider-agnostic; read
  // with an own-key guard so a catalog id named after a prototype member can't
  // resolve to an inherited value.
  const statusOverrides: Record<string, ConnectorStatusOverride> = {
    github: { status: githubStatus },
    slack: { status: slackStatus, identity: slackTeam },
  };
  const overrideFor = (id: string): ConnectorStatusOverride | undefined =>
    Object.hasOwn(statusOverrides, id) ? statusOverrides[id] : undefined;

  return (
    <Section className="space-y-xl" data-testid="integrations-connectors">
      {catalog.length > 0 ? <CollectionToolbar label="Search integrations" value={search} onChange={setSearch} summary={`${matches.length} of ${catalog.length} integrations`} /> : null}
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
        <Grid columns="two" className="grid-cols-1 lg:grid-cols-2">
          {matches.map((entry) => (
            <OAuthConnectorRow
              key={entry.id}
              entry={entry}
              workspaceId={workspaceId}
              override={overrideFor(entry.id)}
            />
          ))}
        </Grid>
      )}
      {catalog.length > 0 && matches.length === 0 ? <EmptyState title="No matching integrations" description="Try another service name or clear your search." /> : null}
    </Section>
  );
}

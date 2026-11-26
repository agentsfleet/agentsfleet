/**
 * The workspace a journey needing a connected app runs in.
 *
 * A connection is one workspace's, and no fixture can make one: it ends on the
 * provider's own consent screen. The fixture person is a member of the
 * workspace that holds the GitHub and Slack connections, which need not be
 * their own, so a journey asks each of their workspaces' connector catalogue
 * and takes the first that shows its provider connected. The journey then
 * creates its fleet beside other fleets there, and deletes only that one.
 */
import type { ConnectorCatalogEntry } from "@/lib/api/connectors";
import { clientFor } from "./api-client";
import type { FixtureKey } from "./constants";
import { listWorkspaces } from "./seed";

/** A connector by its catalogue id, and the provider's name a failure sentence uses. */
export interface Connector {
  readonly id: string;
  readonly provider: string;
}

/** The first of `key`'s workspaces whose connector catalogue shows `connector` connected. */
export async function connectedWorkspaceId(key: FixtureKey, connector: Connector): Promise<string> {
  for (const workspace of await listWorkspaces(key)) {
    const catalogue = await clientFor(key).get<ConnectorCatalogEntry[]>(`/v1/workspaces/${workspace.id}/connectors`);
    if (catalogue.some((row) => row.id === connector.id && row.connected)) return workspace.id;
  }
  throw new Error(`No workspace of the fixture person has connected ${connector.provider}. ` +
    `A person connects ${connector.provider} once from Connected apps in a workspace the fixture person belongs to.`);
}

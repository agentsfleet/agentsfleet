// Shared path ownership keeps authenticated requests inside one fleet.
export function fleetsPath(workspaceId: string): string {
  return `/v1/workspaces/${encodeURIComponent(workspaceId)}/fleets`;
}

export function fleetPath(workspaceId: string, fleetId: string): string {
  return `${fleetsPath(workspaceId)}/${encodeURIComponent(fleetId)}`;
}

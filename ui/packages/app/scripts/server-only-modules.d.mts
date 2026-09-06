export type ServerOnlyModuleMarker = { readonly name: string; readonly marker: string; readonly source?: string };
export function findServerOnlyModule(
  chunks: ReadonlyMap<string, string>,
  markers: ReadonlyArray<ServerOnlyModuleMarker>,
): { file: string; name: string } | null;
export function findStaleMarker(
  runtimes: ReadonlyMap<string, string>,
  markers: ReadonlyArray<ServerOnlyModuleMarker>,
): { name: string; marker: string } | null;

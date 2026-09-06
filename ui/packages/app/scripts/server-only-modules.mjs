// The server-only module guard `.size-limit.mjs` runs after `next build`:
// a marker only that module's runtime carries must be present in the installed
// runtime (so a renamed marker fails loudly) and absent from every client
// chunk (so the module never reaches the browser). Pure over the sources it
// is handed; the caller reads the files.

/** The first client chunk carrying a server-only module, or null. */
export function findServerOnlyModule(chunks, markers) {
  for (const [file, source] of chunks) {
    for (const { name, marker } of markers) {
      if (source.includes(marker)) return { file, name };
    }
  }
  return null;
}

/** The first marker its own runtime no longer carries, or null. */
export function findStaleMarker(runtimes, markers) {
  for (const { name, marker } of markers) {
    const runtime = runtimes.get(name);
    if (runtime === undefined || !runtime.includes(marker)) return { name, marker };
  }
  return null;
}

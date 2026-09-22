import { listWorkspaceLibraryEntriesCached } from "@/lib/api/fleet-library";

// Server-only read wrapper for the Library page, mirroring the convention in
// secrets/lib/reads.ts. The cache() call itself lives beside the client in
// lib/api/fleet-library.ts, which is where the gallery's is too.
//
// MUST NOT be imported from a client component — cache() only exists on the
// server, and a client import would break the build.
export { listWorkspaceLibraryEntriesCached as listLibraryEntriesCached };

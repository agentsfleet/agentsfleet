const LIBRARY_WRITE_SCOPE = "library:write";

type ClaimsWithScopes = {
  scopes?: unknown;
};

export function hasLibraryWriteScope(sessionClaims: unknown): boolean {
  if (!sessionClaims || typeof sessionClaims !== "object") return false;
  const scopes = (sessionClaims as ClaimsWithScopes).scopes;
  if (typeof scopes === "string") {
    return scopes.split(/\s+/).includes(LIBRARY_WRITE_SCOPE);
  }
  if (Array.isArray(scopes)) {
    return scopes.includes(LIBRARY_WRITE_SCOPE);
  }
  return false;
}

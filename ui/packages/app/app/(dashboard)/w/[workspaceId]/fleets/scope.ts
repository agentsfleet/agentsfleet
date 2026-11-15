const PLUGIN_WRITE_SCOPE = "plugins:write";

type ClaimsWithScopes = {
  scopes?: unknown;
};

export function hasPluginWriteScope(sessionClaims: unknown): boolean {
  if (!sessionClaims || typeof sessionClaims !== "object") return false;
  const scopes = (sessionClaims as ClaimsWithScopes).scopes;
  if (typeof scopes === "string") {
    return scopes.split(/\s+/).includes(PLUGIN_WRITE_SCOPE);
  }
  if (Array.isArray(scopes)) {
    return scopes.includes(PLUGIN_WRITE_SCOPE);
  }
  return false;
}

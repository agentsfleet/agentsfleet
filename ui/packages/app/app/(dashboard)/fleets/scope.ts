const TEMPLATE_WRITE_SCOPE = "template:write";

type ClaimsWithScopes = {
  scopes?: unknown;
};

export function hasTemplateWriteScope(sessionClaims: unknown): boolean {
  if (!sessionClaims || typeof sessionClaims !== "object") return false;
  const scopes = (sessionClaims as ClaimsWithScopes).scopes;
  if (typeof scopes === "string") {
    return scopes.split(/\s+/).includes(TEMPLATE_WRITE_SCOPE);
  }
  if (Array.isArray(scopes)) {
    return scopes.includes(TEMPLATE_WRITE_SCOPE);
  }
  return false;
}

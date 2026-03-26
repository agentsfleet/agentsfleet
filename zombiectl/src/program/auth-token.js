function decodeTokenPayload(token) {
  if (!token || typeof token !== "string") return null;
  const parts = token.split(".");
  if (parts.length < 2 || !parts[1]) return null;
  try {
    const base64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const padded = base64 + "===".slice((base64.length + 3) % 4);
    return JSON.parse(Buffer.from(padded, "base64").toString("utf8"));
  } catch {
    return null;
  }
}

function extractDistinctIdFromToken(token) {
  const payload = decodeTokenPayload(token);
  if (payload && typeof payload.sub === "string" && payload.sub.trim().length > 0) {
    return payload.sub.trim();
  }
  return null;
}

function extractRoleFromToken(token) {
  const payload = decodeTokenPayload(token);
  if (!payload) return null;

  const candidates = [
    payload.role,
    payload.metadata?.role,
    payload.custom_claims?.role,
    payload.app_metadata?.role,
    payload["https://usezombie.dev/role"],
    payload["https://usezombie.com/role"],
    payload.metadata?.["https://usezombie.dev/role"],
    payload.metadata?.["https://usezombie.com/role"],
  ];
  for (const raw of candidates) {
    if (typeof raw !== "string") continue;
    const value = raw.toLowerCase();
    if (value === "user" || value === "operator" || value === "admin") return value;
  }
  return null;
}

export { decodeTokenPayload, extractDistinctIdFromToken, extractRoleFromToken };

function extractDistinctIdFromToken(token) {
  if (!token || typeof token !== "string") return null;
  const parts = token.split(".");
  if (parts.length < 2 || !parts[1]) return null;
  try {
    const base64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const padded = base64 + "===".slice((base64.length + 3) % 4);
    const payload = JSON.parse(Buffer.from(padded, "base64").toString("utf8"));
    if (payload && typeof payload.sub === "string" && payload.sub.trim().length > 0) {
      return payload.sub.trim();
    }
  } catch {
    return null;
  }
  return null;
}

export { extractDistinctIdFromToken };

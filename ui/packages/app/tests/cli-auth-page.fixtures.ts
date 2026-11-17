export function activeBody(overrides: Partial<{ status: string; cli_public_key: string; token_name: string; expires_at: string }> = {}) {
  return {
    status: "pending",
    cli_public_key: "AAAA",
    token_name: "darwin/arm64",
    expires_at: new Date(Date.now() + 300_000).toISOString(),
    ...overrides,
  };
}

export function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

export async function samplePeerSpkiBase64Url(): Promise<string> {
  const pair = await crypto.subtle.generateKey(
    { name: "ECDH", namedCurve: "P-256" },
    true,
    ["deriveBits"],
  );
  const spki = await crypto.subtle.exportKey("spki", pair.publicKey);
  const bytes = new Uint8Array(spki);
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

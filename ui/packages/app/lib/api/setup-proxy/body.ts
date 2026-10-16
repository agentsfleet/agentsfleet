// Incoming bodies stop at the daemon's native envelope limit, even without length.

export const SETUP_BODY_LIMIT_BYTES = 128 * 1024;
export const JSON_MEDIA_TYPE = "application/json";

export function sameOrigin(request: Request): boolean {
  const origin = request.headers.get("origin");
  if (!origin || origin === "null") return false;
  const url = new URL(request.url);
  const host = request.headers.get("x-forwarded-host")?.split(",")[0]?.trim() || request.headers.get("host") || url.host;
  const protocol = request.headers.get("x-forwarded-proto")?.split(",")[0]?.trim() || url.protocol.slice(0, -1);
  try { return new URL(origin).origin === `${protocol}://${host}`; } catch { return false; }
}

export async function boundedBody(request: Request, signal: AbortSignal): Promise<Uint8Array<ArrayBuffer> | null> {
  if (Number(request.headers.get("content-length") ?? 0) > SETUP_BODY_LIMIT_BYTES) return null;
  const reader = request.body?.getReader();
  if (!reader) return new Uint8Array();
  const chunks: Uint8Array[] = [];
  let size = 0;
  const cancel = (): void => { void reader.cancel().catch(() => undefined); };
  signal.addEventListener("abort", cancel, {once: true});
  try {
    signal.throwIfAborted();
    for (let chunk = await reader.read(); !chunk.done; chunk = await reader.read()) {
      size += chunk.value.byteLength;
      if (size > SETUP_BODY_LIMIT_BYTES) { await reader.cancel(); return null; }
      chunks.push(chunk.value);
    }
    signal.throwIfAborted();
    const body = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) { body.set(chunk, offset); offset += chunk.byteLength; }
    return body;
  } catch (cause) {
    if (signal.aborted) return null;
    throw cause;
  } finally { signal.removeEventListener("abort", cancel); reader.releaseLock(); }
}

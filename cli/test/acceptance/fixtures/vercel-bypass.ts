/** Exchange deployment protection headers before any browser requests exist. */
import type { BrowserContext } from "playwright";
import { acceptanceProxy } from "./proxy.ts";

type StorageState = Awaited<ReturnType<BrowserContext["storageState"]>>;

export async function vercelBypassState(origin: string, secret?: string): Promise<StorageState> {
  if (!secret) return { cookies: [], origins: [] };
  const { request } = await import("playwright");
  const proxy = acceptanceProxy();
  const context = await request.newContext({
    ...(proxy === undefined ? {} : {proxy}),
    extraHTTPHeaders: {
      "x-vercel-protection-bypass": secret,
      "x-vercel-set-bypass-cookie": "true",
    },
  });
  try {
    // Never follow a redirect with the raw bypass secret. Only the derived
    // same-origin cookie enters the browser, leaving Clerk requests untouched.
    await context.get(origin, { maxRedirects: 0 });
    return await context.storageState();
  } finally {
    await context.dispose();
  }
}

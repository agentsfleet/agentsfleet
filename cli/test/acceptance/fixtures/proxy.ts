import type { LaunchOptions } from "playwright";

/** Browser requests and Clerk's request context use the same configured route. */
export function acceptanceProxy(env: NodeJS.ProcessEnv = process.env): LaunchOptions["proxy"] {
  const server = env.HTTPS_PROXY ?? env.https_proxy;
  if (!server) return undefined;
  const bypass = env.NO_PROXY ?? env.no_proxy;
  return {server, ...(bypass === undefined ? {} : {bypass})};
}

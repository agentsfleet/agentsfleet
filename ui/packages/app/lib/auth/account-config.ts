/** Server-only configuration; the existing key is optional until personal settings are used. */
export const ACCOUNT_PROVIDER_CONFIG = Object.freeze({
  secretKey: process.env.CLERK_SECRET_KEY,
  apiRoot: "https://api.clerk.com/v1",
  apiVersion: "2026-05-12",
  bodyLimit: 1024 * 1024,
});

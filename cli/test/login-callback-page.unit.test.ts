import { describe, expect, test } from "bun:test";
import { BRAND_MARK_PATH, PAGE_STATUS, callbackPage } from "../src/commands/auth/login-callback-page.ts";

describe("the login callback page", () => {
  test("renders the approved completion message", async () => {
    const html = await callbackPage(PAGE_STATUS.success).text();
    expect(html).toContain("<h1>Unleash your agents.</h1>");
    expect(html).toContain("You're signed in. Return to your terminal and put your agents to work.");
    expect(html).toContain("You can close this tab.");
  });

  test("every outcome carries the mark, its heading and an icon, and loads nothing", async () => {
    for (const status of Object.values(PAGE_STATUS)) {
      const html = await callbackPage(status).text();
      expect(html).toContain(`d="${BRAND_MARK_PATH}"`);
      expect(html).toContain(`class="status ${status}"`);
      expect(html).toMatch(/<h1>[^<]+<\/h1>/);
      expect(html).not.toMatch(/<(img|link|script)\b/);
      expect(html).not.toMatch(/\b(src|href)=/);
    }
  });

  // Every directive, spelled out: dropping any one of them must fail here.
  test("keeps the strict content policy on every outcome", () => {
    for (const status of Object.values(PAGE_STATUS)) {
      const policy = callbackPage(status).headers.get("Content-Security-Policy");
      expect(policy?.split("; ").sort()).toEqual([
        "base-uri 'none'",
        "default-src 'none'",
        "form-action 'none'",
        "frame-ancestors 'none'",
        "style-src 'unsafe-inline'",
      ]);
    }
  });
});

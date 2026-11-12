import { describe, expect, test } from "bun:test";
import { BRAND_MARK_PATH, PAGE_STATUS, callbackPage } from "../src/commands/auth/login-callback-page.ts";

describe("the login callback page", () => {
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

  test("keeps the strict content policy", () => {
    const policy = callbackPage(PAGE_STATUS.success).headers.get("Content-Security-Policy");
    expect(policy).toContain("default-src 'none'");
  });
});

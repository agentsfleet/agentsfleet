import { test, expect } from "@playwright/test";
import { LLMS_FULL_INTRO } from "../../src/lib/llms-text";

/**
 * Smoke tests — fast subset for CI pre-deploy gate.
 * Run with: bunx playwright test tests/e2e/smoke.spec.ts
 * For post-deploy Vercel: BASE_URL=https://agentsfleet.net bunx playwright test tests/e2e/smoke.spec.ts
 */
test.describe("Smoke", () => {
  test("home page loads", async ({ page }) => {
    await page.goto("/");
    await expect(page).toHaveTitle(/agentsfleet/i);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  });

  test("brand favicon is wired and resolves", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator('link[rel="icon"][type="image/svg+xml"]')).toHaveAttribute("href", "/favicon.svg");
    const res = await page.request.get("/favicon.svg");
    expect(res.status()).toBe(200);
    expect(res.headers()["content-type"]).toContain("image/svg+xml");
    expect(await res.text()).toContain("<svg");
  });

  test("brand meta description names agentsfleet", async ({ page }) => {
    await page.goto("/");
    const description = await page
      .locator('meta[name="description"]')
      .getAttribute("content");
    expect(description).toContain("agentsfleet");
  });

  test("pricing section renders inline on home", async ({ page }) => {
    await page.goto("/#pricing");
    await expect(page.getByTestId("pricing-block")).toBeVisible();
    await expect(page.getByTestId("pricing-cta-early-access")).toBeVisible();
    // pin test: approved pricing and direct provider-billing wording.
    await expect(page.getByTestId("pricing-block")).toContainText("If you bring your own model key, your provider bills model usage directly");
    await expect(page.getByTestId("pricing-block")).toContainText("Fleet runtime starts at $0.");
    await expect(page.getByTestId("pricing-block")).not.toContainText(/credit|\$25|start free|unlimited/i);
  });

  test("llms surfaces are generated and reachable", async ({ page }) => {
    const llms = await page.request.get("/llms.txt");
    expect(llms.status()).toBe(200);
    const llmsBody = await llms.text();
    expect(llmsBody).toContain("# agentsfleet");
    expect(llmsBody).toContain("## Product");
    expect(llmsBody).toContain("[OpenAPI](/openapi.json)");

    const llmsFull = await page.request.get("/llms-full.txt");
    expect(llmsFull.status()).toBe(200);
    const llmsFullBody = await llmsFull.text();
    expect(llmsFullBody).toContain("## Pillars");
    expect(llmsFullBody).toContain(LLMS_FULL_INTRO);
  });

  test("fleets page loads", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for agents.",
    );
  });

  test("privacy page loads", async ({ page }) => {
    await page.goto("/privacy");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Privacy Policy");
  });

  test("terms page loads", async ({ page }) => {
    await page.goto("/terms");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Terms of Service");
  });

  test("nav links are present on home", async ({ page }) => {
    await page.goto("/");
    const nav = page.getByRole("navigation", { name: /primary/i });
    await expect(nav.getByRole("link", { name: /^Product$/i })).toBeVisible();
    await expect(nav.getByRole("link", { name: /^Pricing$/i })).toBeVisible();
    await expect(nav.getByRole("link", { name: /^GitHub$/ })).toBeVisible();
    await expect(nav.getByRole("link", { name: /^docs$/i })).toBeVisible();
    await expect(page.getByRole("link", { name: /^sign in$/i })).toHaveCount(0);
  });

  test("footer renders on all routes", async ({ page }) => {
    for (const route of ["/", "/agents", "/privacy", "/terms"]) {
      await page.goto(route);
      await page.waitForLoadState("domcontentloaded");
      await expect(page.getByRole("contentinfo")).toBeVisible({ timeout: 10_000 });
    }
  });

  test("Discord link uses canonical URL", async ({ page }) => {
    await page.goto("/");
    const discord = page.getByRole("contentinfo").getByRole("link", { name: /^discord$/i });
    await expect(discord).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
  });

  test("footer offers direct contact", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("contentinfo").getByRole("link", { name: /^contact$/i }))
      .toHaveAttribute("href", "mailto:agentsfleet@agentmail.to");
  });

  test("reduced motion leaves the investigation static", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/");
    await expect(page.getByRole("button", { name: /pause demo/i })).toHaveCount(0);
    for (const [role, name] of [
      ["region", "Slack ticket in customer-support"],
      ["article", "Scout investigates the recurring chat freeze"],
      ["region", "Draft fix in the Slack thread"],
      ["region", "Verified result in the Slack thread"],
    ] as const) {
      await expect(page.getByRole(role, { name })).toBeVisible();
    }
    await expect(page.getByRole("region", { name: "Verified result in the Slack thread" })).toContainText("95% of replies started within 2.8 seconds, down from 15.1 seconds.");
  });

  test("the incident stays readable if the playback script fails", async ({ page }) => {
    await page.route("**/*", (route) =>
      route.request().resourceType() === "script" ? route.abort() : route.continue(),
    );
    await page.goto("/");
    await expect(page.getByRole("button", { name: /pause demo/i })).toHaveCount(0);
    await expect(page.getByRole("region", { name: "Slack ticket in customer-support" })).toBeVisible();
    await expect(page.getByRole("article", { name: "Scout investigates the recurring chat freeze" })).toBeVisible();
    const story = page.getByRole("region", { name: "Support investigation example" });
    const result = story.getByRole("region", { name: "Verified result in the Slack thread" });
    await story.focus();
    await expect(story).toBeFocused();
    await result.scrollIntoViewIfNeeded();
    await expect(result).toBeInViewport();
  });

  test("the story keeps its space when playback starts", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 900 });
    let startScripts!: () => void;
    const scriptsAllowed = new Promise<void>((resolve) => { startScripts = resolve; });
    await page.route("**/*", async (route) => {
      if (route.request().resourceType() === "script") await scriptsAllowed;
      await route.continue();
    });
    const story = page.getByRole("figure", { name: "Why did chat freeze?" });
    try {
      await page.goto("/", { waitUntil: "commit" });
      await expect(story.getByText("Scroll across to read the full story.")).toBeVisible();
      await page.evaluate(() => document.fonts.ready);
      const before = await story.boundingBox();
      startScripts();
      await expect(story.getByRole("button", { name: "Pause demo" })).toBeVisible();
      const after = await story.boundingBox();
      expect(Math.abs(after!.height - before!.height)).toBeLessThanOrEqual(16);
      await expect(story.locator("[data-active=false]").first()).toHaveCSS("transition-duration", "0s");
    } finally {
      startScripts();
    }
  });
});

test.describe("Crawler view", () => {
  test.use({ javaScriptEnabled: false });
  test("the homepage explains the incident and exposes its answers without JavaScript", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("region", { name: "Slack ticket in customer-support" })).toContainText("@agentfleet");
    await expect(page.getByRole("article", { name: "Scout investigates the recurring chat freeze" })).toContainText("Grafana");
    await expect(page.getByRole("region", { name: "Verified result in the Slack thread" })).toContainText("95% of replies started within 2.8 seconds, down from 15.1 seconds.");
    const answers = await page.locator("#faq article").evaluateAll(items => items.map(item => ({
      question: item.querySelector("h3")!.innerText,
      answer: item.querySelector("p")!.innerText,
    })));
    const schema = JSON.parse((await page.locator('script[type="application/ld+json"]').textContent())!);
    const faq = schema["@graph"].find((entry: { "@type": string }) => entry["@type"] === "FAQPage");
    expect(answers.length).toBeGreaterThan(0);
    expect(faq.mainEntity.map((entry: { name: string; acceptedAnswer: { text: string } }) => ({
      question: entry.name, answer: entry.acceptedAnswer.text,
    }))).toEqual(answers);
  });
});

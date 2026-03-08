import { test, expect } from "@playwright/test";

/**
 * Navigation edge case tests.
 * Covers: footer links, SPA routing, mode toggle from all routes,
 * multi-step flows, and direct URL navigation.
 */

test.describe("Footer navigation", () => {
  test("footer Agents link navigates to /agents and activates Agents mode", async ({ page }) => {
    await page.goto("/");
    // Humans mode should be active initially
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);

    // Click Agents in footer
    await page.getByRole("contentinfo").getByRole("link", { name: "Agents" }).click();

    await expect(page).toHaveURL(/\/agents/);
    await expect(page.getByTestId("mode-agents")).toHaveClass(/active/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("autonomous agents");
  });

  test("footer Pricing link navigates to /pricing and keeps Humans mode active", async ({ page }) => {
    await page.goto("/agents");
    // Agents mode active
    await expect(page.getByTestId("mode-agents")).toHaveClass(/active/);

    await page.getByRole("contentinfo").getByRole("link", { name: "Pricing" }).click();

    await expect(page).toHaveURL(/\/pricing/);
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Free and Scale plans");
  });

  test("footer Features link navigates to / and Humans mode is active", async ({ page }) => {
    await page.goto("/agents");
    await page.getByRole("contentinfo").getByRole("link", { name: "Features" }).click();
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);
  });

  test("footer Privacy link navigates to /privacy", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("contentinfo").getByRole("link", { name: "Privacy" }).click();
    await expect(page).toHaveURL(/\/privacy/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Privacy Policy");
  });

  test("footer Terms link navigates to /terms", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("contentinfo").getByRole("link", { name: "Terms" }).click();
    await expect(page).toHaveURL(/\/terms/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Terms of Service");
  });

  test("footer Discord link has canonical URL and opens in new tab", async ({ page }) => {
    await page.goto("/");
    const discord = page.getByRole("contentinfo").getByRole("link", { name: "Discord" });
    await expect(discord).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
    await expect(discord).toHaveAttribute("target", "_blank");
    await expect(discord).toHaveAttribute("rel", "noopener noreferrer");
  });

  test("footer GitHub link opens in new tab", async ({ page }) => {
    await page.goto("/");
    const github = page.getByRole("contentinfo").getByRole("link", { name: "GitHub" });
    await expect(github).toHaveAttribute("target", "_blank");
    await expect(github).toHaveAttribute("rel", "noopener noreferrer");
  });
});

test.describe("Mode toggle — all routes", () => {
  test("toggle Agents from /pricing → goes to /agents", async ({ page }) => {
    await page.goto("/pricing");
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);

    await page.getByTestId("mode-agents").click();

    await expect(page).toHaveURL(/\/agents/);
    await expect(page.getByTestId("mode-agents")).toHaveClass(/active/);
  });

  test("toggle Humans from /agents → goes to /", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByTestId("mode-agents")).toHaveClass(/active/);

    await page.getByTestId("mode-humans").click();

    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);
  });

  test("toggle Humans from /pricing stays on / (not /pricing)", async ({ page }) => {
    await page.goto("/pricing");
    await page.getByTestId("mode-humans").click();
    // Humans mode was already active but click should still navigate to /
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
  });

  test("toggle Humans from /privacy → goes to /", async ({ page }) => {
    await page.goto("/privacy");
    await page.getByTestId("mode-humans").click();
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
  });

  test("toggle Agents from /privacy → goes to /agents", async ({ page }) => {
    await page.goto("/privacy");
    await page.getByTestId("mode-agents").click();
    await expect(page).toHaveURL(/\/agents/);
  });
});

test.describe("Mode toggle — multi-step cycles", () => {
  test("/ → Agents → Humans → Agents → Humans (no state corruption)", async ({ page }) => {
    await page.goto("/");

    // Cycle 1
    await page.getByTestId("mode-agents").click();
    await expect(page).toHaveURL(/\/agents/);
    await expect(page.getByTestId("mode-agents")).toHaveClass(/active/);

    await page.getByTestId("mode-humans").click();
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);

    // Cycle 2
    await page.getByTestId("mode-agents").click();
    await expect(page).toHaveURL(/\/agents/);
    await expect(page.getByTestId("mode-agents")).toHaveClass(/active/);

    await page.getByTestId("mode-humans").click();
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Ship AI-generated PRs");
  });

  test("pricing → home → agents → home flow", async ({ page }) => {
    await page.goto("/pricing");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Free and Scale plans");

    // Nav to home
    await page.getByRole("navigation", { name: /primary/i }).getByRole("link", { name: "Home" }).click();
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);

    // Toggle to agents
    await page.getByTestId("mode-agents").click();
    await expect(page).toHaveURL(/\/agents/);
    await expect(page.getByTestId("mode-agents")).toHaveClass(/active/);

    // Toggle back to humans
    await page.getByTestId("mode-humans").click();
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Ship AI-generated PRs");
  });

  test("home nav Pricing → /pricing → back to home via nav", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("navigation", { name: /primary/i }).getByRole("link", { name: "Pricing" }).click();
    await expect(page).toHaveURL(/\/pricing/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Free and Scale plans");

    await page.getByRole("navigation", { name: /primary/i }).getByRole("link", { name: "Home" }).click();
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Ship AI-generated PRs");
  });
});

test.describe("Direct URL navigation — mode consistency", () => {
  test("direct nav to /agents shows Agents mode active without clicking toggle", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByTestId("mode-agents")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("mode-agents")).toHaveClass(/active/);
    await expect(page.getByTestId("mode-humans")).toHaveAttribute("aria-selected", "false");
  });

  test("direct nav to /pricing shows Humans mode active", async ({ page }) => {
    await page.goto("/pricing");
    await expect(page.getByTestId("mode-humans")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);
  });

  test("direct nav to /privacy shows Humans mode active", async ({ page }) => {
    await page.goto("/privacy");
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);
  });

  test("direct nav to /terms shows Humans mode active", async ({ page }) => {
    await page.goto("/terms");
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);
  });

  test("browser back button restores correct mode after toggle", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("mode-agents").click();
    await expect(page).toHaveURL(/\/agents/);

    await page.goBack();
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);
  });
});

test.describe("SPA routing — no full page reloads", () => {
  test("header Pricing navigates without reload", async ({ page }) => {
    await page.goto("/");
    const navEvents: string[] = [];
    page.on("framenavigated", (frame) => {
      if (frame === page.mainFrame()) navEvents.push(frame.url());
    });

    await page.getByRole("navigation", { name: /primary/i }).getByRole("link", { name: "Pricing" }).click();
    await expect(page).toHaveURL(/\/pricing/);

    // SPA navigation — only the initial load should be in navEvents
    // (framenavigated fires on initial goto and hash changes, not pushState)
    expect(navEvents.length).toBeLessThanOrEqual(1);
  });

  test("footer Agents link is a real anchor (React Router Link)", async ({ page }) => {
    await page.goto("/");
    const agentsLink = page.getByRole("contentinfo").getByRole("link", { name: "Agents" });
    await expect(agentsLink).toHaveAttribute("href", "/agents");
  });

  test("footer Pricing link is a real anchor (React Router Link)", async ({ page }) => {
    await page.goto("/");
    const pricingLink = page.getByRole("contentinfo").getByRole("link", { name: "Pricing" });
    await expect(pricingLink).toHaveAttribute("href", "/pricing");
  });
});

test.describe("Agents page — install block", () => {
  test("Install Zombiectl block is visible on /agents", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("heading", { name: "Install Zombiectl" })).toBeVisible();
  });

  test("curl command is readable in install block", async ({ page }) => {
    await page.goto("/agents");
    const block = page.getByLabel(/install zombiectl command/i);
    await expect(block).toContainText("curl -sSL https://usezombie.sh/install | bash");
  });

  test("Read the docs button links to docs", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("link", { name: "Read the docs" })).toHaveAttribute(
      "href",
      "https://docs.usezombie.com"
    );
  });

  test("Setup your personal dashboard button is double-bordered", async ({ page }) => {
    await page.goto("/agents");
    const btn = page.getByRole("link", { name: "Setup your personal dashboard" });
    await expect(btn).toBeVisible();
    await expect(btn).toHaveClass(/z-btn--double/);
  });
});

import { test, expect, type Locator } from "@playwright/test";

// Operational Restraint design tokens (M64_002). Hex sources live in
// ui/packages/design-system/src/tokens.css; rgb() values below are the
// browser-computed forms of those hex codes.
//   --surface-1 #11161a → rgb(17, 22, 26)  — cards, sidebars
//   --surface-2 #181e22 → rgb(24, 30, 34)  — inputs, mockup chrome
//   --text      #e6eaec → rgb(230, 234, 236)
//   --text-muted #8b9398 → rgb(139, 147, 152)
const DARK_SURFACE = "rgb(17, 22, 26)";
const ELEVATED_SURFACE = "rgb(24, 30, 34)";
const PRIMARY_TEXT = "rgb(230, 234, 236)";
const MUTED_TEXT = "rgb(139, 147, 152)";
const BASE_URL = process.env.BASE_URL ?? "http://localhost:3000";
const EXPECTED_HOSTNAME = new URL(BASE_URL).hostname;

async function getCss(locator: Locator, property: string) {
  return locator.evaluate(
    (element, cssProperty) => window.getComputedStyle(element).getPropertyValue(cssProperty),
    property,
  );
}

test.describe("Auth theming", () => {
  test("sign-in uses dashboard token colors", async ({ page }) => {
    await page.goto("/sign-in");

    const heading = page.getByRole("heading", { level: 1, name: /sign in/i });
    const subtitle = page.getByText("Welcome back! Please sign in to continue");
    const label = page.getByText("Email address", { exact: true });
    const button = page.getByRole("button", { name: /continue/i }).last();

    await expect(heading).toBeVisible();
    await expect(button).toBeVisible();

    expect(await getCss(heading, "color")).toBe(PRIMARY_TEXT);
    expect(await getCss(subtitle, "color")).toBe(MUTED_TEXT);
    expect(await getCss(label, "color")).toBe(PRIMARY_TEXT);
    expect(await getCss(button, "background-color")).not.toBe("rgb(109, 74, 255)");
  });

  test("protected route redirects to local sign-in instead of hosted clerk", async ({ page }) => {
    await page.goto("/agents");
    await page.waitForTimeout(MS_PER_SECOND);

    expect(new URL(page.url()).hostname).toBe(EXPECTED_HOSTNAME);
    expect(page.url()).not.toContain("accounts.dev");

    // Clerk dev-keys enforce strict rate limits; when exhausted mid-suite,
    // Clerk injects a "Temporary API keys" configuration widget at the same
    // URL, defeating the body-text/bg-color assertions below. Detect and
    // skip rather than flake.
    const bodyText = (await page.locator("body").textContent()) ?? "";
    test.skip(
      bodyText.includes("Temporary API keys"),
      "Clerk dev-keys rate-limited — interstitial served in place of app sign-in",
    );

    const body = page.locator("body");
    await expect(body).toContainText(/usezombie|Agents|Dashboard/);
    // --bg #0a0d0e (Operational Restraint) → rgb(10, 13, 14)
    expect(await getCss(body, "background-color")).toBe("rgb(10, 13, 14)");
  });

  test("mobile auth keeps the dark card treatment", async ({ page, isMobile }) => {
    test.skip(!isMobile, "Mobile-only assertion");

    await page.goto("/sign-in");

    const heading = page.getByRole("heading", { level: 1, name: /sign in/i });
    const googleButton = page.getByRole("button", { name: /continue with google/i });

    await expect(heading).toBeVisible();
    await expect(googleButton).toBeVisible();

    const buttonBackground = await getCss(googleButton, "background-color");
    expect(await getCss(heading, "color")).toBe(PRIMARY_TEXT);
    expect([DARK_SURFACE, ELEVATED_SURFACE]).toContain(buttonBackground);
    expect(buttonBackground).not.toBe("rgb(255, 255, 255)");
  });
});
const MS_PER_SECOND = 1000 as const;

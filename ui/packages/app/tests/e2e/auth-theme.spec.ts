import { test, expect, type Locator } from "@playwright/test";

const DARK_SURFACE = "rgb(15, 21, 32)";
const ELEVATED_SURFACE = "rgb(22, 30, 43)";
const PRIMARY_TEXT = "rgb(232, 242, 255)";
const MUTED_TEXT = "rgb(139, 151, 168)";
const BASE_URL = process.env.BASE_URL ?? "http://localhost:3000";
const EXPECTED_HOSTNAME = new URL(BASE_URL).hostname;

async function getCss(locator: Locator, property: string) {
  return locator.evaluate(
    (element, cssProperty) => window.getComputedStyle(element).getPropertyValue(cssProperty),
    property,
  );
}

test.describe("Auth theming", () => {
  test("sign-in uses mission control token colors", async ({ page }) => {
    await page.goto("/sign-in");

    const heading = page.getByRole("heading", { level: 1, name: /sign in/i });
    const subtitle = page.getByText("Welcome back! Please sign in to continue");
    const label = page.getByText("Email address", { exact: true });
    const button = page.getByRole("button", { name: /continue/i }).last();

    await expect(heading).toBeVisible();
    await expect(button).toBeVisible();

    await expect(await getCss(heading, "color")).toBe(PRIMARY_TEXT);
    await expect(await getCss(subtitle, "color")).toBe(MUTED_TEXT);
    await expect(await getCss(label, "color")).toBe(PRIMARY_TEXT);
    await expect(await getCss(button, "background-color")).not.toBe("rgb(109, 74, 255)");
  });

  test("protected route redirects to local sign-in instead of hosted clerk", async ({ page }) => {
    await page.goto("/workspaces");
    await page.waitForTimeout(1000);

    expect(new URL(page.url()).hostname).toBe(EXPECTED_HOSTNAME);
    expect(page.url()).not.toContain("accounts.dev");

    const body = page.locator("body");
    await expect(body).toContainText(/UseZombie|Workspaces/);
    await expect(await getCss(body, "background-color")).toBe("rgb(5, 8, 13)");
  });

  test("mobile auth keeps the dark card treatment", async ({ page, isMobile }) => {
    test.skip(!isMobile, "Mobile-only assertion");

    await page.goto("/sign-in");

    const heading = page.getByRole("heading", { level: 1, name: /sign in/i });
    const googleButton = page.getByRole("button", { name: /continue with google/i });

    await expect(heading).toBeVisible();
    await expect(googleButton).toBeVisible();

    const buttonBackground = await getCss(googleButton, "background-color");
    await expect(await getCss(heading, "color")).toBe(PRIMARY_TEXT);
    expect([DARK_SURFACE, ELEVATED_SURFACE]).toContain(buttonBackground);
    await expect(buttonBackground).not.toBe("rgb(255, 255, 255)");
  });
});

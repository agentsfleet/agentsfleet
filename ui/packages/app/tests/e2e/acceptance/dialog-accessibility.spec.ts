import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";

const WIDTHS = [1440, 390, 320];
const CREATE_LABEL = "Create";
const CREATE_RUNNER = "Create runner";
const HOST_ERROR = "1–256 characters: letters, digits, dot, hyphen, underscore";

for (const width of WIDTHS) {
  test(`runner validation and expanded mounts keep the footer accessible at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await signInAs(page, "operator");
    await page.goto("/admin/runners");
    const opener = page.getByRole("button", { name: CREATE_RUNNER, exact: true });
    await opener.click();
    const dialog = page.getByRole("dialog", { name: CREATE_RUNNER });
    await expect(dialog).toHaveAccessibleDescription(/.+/);
    await expectAccessible(page, "Create runner");
    await dialog.getByRole("button", { name: "Sandbox mounts (optional)", exact: true }).click();
    await dialog.getByRole("button", { name: "Add mount", exact: true }).click();
    const path = dialog.getByRole("textbox", { name: "Mount path 1", exact: true });
    await expect(path).toBeVisible();
    await expectAccessible(page, "Expanded sandbox mount");
    const submit = dialog.getByRole("button", { name: CREATE_LABEL, exact: true });
    await submit.scrollIntoViewIfNeeded();
    await expect(submit).toBeInViewport();
    await submit.click();
    await expect(dialog.getByText(HOST_ERROR, { exact: true })).toBeVisible();
    const host = dialog.getByRole("textbox", { name: "Host name", exact: true });
    await expect(host).toHaveAttribute("aria-invalid", "true");
    await expect(host).toHaveAccessibleDescription(new RegExp(HOST_ERROR));
    await expectAccessible(page, "Invalid runner submission");
    await expectNoPageOverflow(page);
    const cancel = dialog.getByRole("button", { name: "Cancel", exact: true });
    await cancel.scrollIntoViewIfNeeded();
    await expect(cancel).toBeInViewport();
    await cancel.click();
    await expect(dialog).toBeHidden();
    await expect(opener).toBeFocused();
  });

  test(`library source tabs and upload footer remain accessible at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await signInAs(page, "operator");
    await page.goto("/admin/fleet-libraries");
    const opener = page.getByRole("button", { name: "Create fleet library", exact: true });
    await opener.click();
    const dialog = page.getByRole("dialog", { name: "Create fleet library" });
    await expect(dialog).toHaveAccessibleDescription(/.+/);
    await expect(dialog.getByRole("tab", { name: "GitHub", exact: true })).toBeVisible();
    await expect(dialog).toHaveCSS("opacity", "1");
    await expectAccessible(page, "GitHub library source");
    const upload = dialog.getByRole("tab", { name: "Upload from computer", exact: true });
    await upload.focus();
    await page.keyboard.press("Enter");
    await expect(upload).toHaveAttribute("aria-selected", "true");
    await expectAccessible(page, "Upload library source");
    const submit = dialog.getByRole("button", { name: CREATE_LABEL, exact: true });
    await submit.scrollIntoViewIfNeeded();
    await expect(submit).toBeInViewport();
    await submit.click();
    await expect(dialog.getByText("Add the SKILL.md body", { exact: true })).toBeVisible();
    await expect(dialog.getByText("Add the TRIGGER.md body", { exact: true })).toBeVisible();
    await expectAccessible(page, "Missing upload documents");
    await expectNoPageOverflow(page);
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(opener).toBeFocused();
  });
}

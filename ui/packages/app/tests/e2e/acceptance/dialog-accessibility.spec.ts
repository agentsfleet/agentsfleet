import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";
import { FIXTURE_KEY } from "./fixtures/constants";
import { withStagedPlatformEntry } from "./fixtures/native-setup";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { ADMIN_FLEET_LIBRARIES_PATH, EDIT, EDIT_NAME_LABEL, EDIT_TITLE } from "@/app/(dashboard)/admin/fleet-libraries/library-copy";

const WIDTHS = [1440, 390, 320];
const CREATE_LABEL = "Create";
const CREATE_RUNNER = "Create runner";
// A constant prefix per width: parallel widths never share a staged entry.
const A11Y_ENTRY_PREFIX = "e2e-dialog-a11y-";
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

  test(`the platform Library edit dialog stays accessible at ${width}px`, async ({ page }) => {
    const slug = `${A11Y_ENTRY_PREFIX}${width}`;
    const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.operator);
    await withStagedPlatformEntry(workspace, slug, async () => {
      await page.setViewportSize({ width, height: 900 });
      await signInAs(page, FIXTURE_KEY.operator);
      await page.goto(ADMIN_FLEET_LIBRARIES_PATH);
      const opener = page.getByRole("row").filter({ hasText: slug }).getByRole("button", { name: EDIT, exact: true });
      await opener.focus();
      await page.keyboard.press("Enter");
      const dialog = page.getByRole("dialog", { name: EDIT_TITLE });
      await expect(dialog).toHaveAccessibleDescription(/.+/);
      await expect(dialog).toHaveCSS("opacity", "1");
      await expectAccessible(page, "Platform Library edit");
      await dialog.getByLabel(EDIT_NAME_LABEL, { exact: true }).fill("");
      const save = dialog.getByRole("button", { name: "Save", exact: true });
      await save.scrollIntoViewIfNeeded();
      await expect(save).toBeInViewport();
      await expect(save).toBeDisabled();
      await expectAccessible(page, "Platform Library edit without a name");
      await expectNoPageOverflow(page);
      await page.keyboard.press("Escape");
      await expect(dialog).toBeHidden();
      await expect(opener).toBeFocused();
    });
  });
}

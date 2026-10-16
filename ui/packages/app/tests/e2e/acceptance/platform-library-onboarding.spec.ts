import { expect, test } from "@playwright/test";
import { z } from "zod";
import { SETUP_HEADERS } from "@/lib/api/draft-types";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { withStagedPlatformEntry } from "./fixtures/native-setup";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import {
  ADMIN_FLEET_LIBRARIES_PATH, DELETE, DELETE_CONFIRM_TITLE, EDIT, EDIT_DESCRIPTION_LABEL, EDIT_TITLE, PUBLISH, UNPUBLISH,
} from "@/app/(dashboard)/admin/fleet-libraries/library-copy";

test("workspace users cannot reach the platform Library editor", async ({page}) => {
  await signInAs(page, FIXTURE_KEY.regular);
  await page.goto(ADMIN_FLEET_LIBRARIES_PATH);
  await expect(page.getByRole("heading", {name: /fleet library/i})).toHaveCount(0);
});

test("a stale deletion leaves its error inside the confirmation and retains the entry", async ({page}) => {
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.operator);
  const name = `platform-delete-${crypto.randomUUID().slice(0, 8)}`;
  const path = "/v1/admin/fleet-libraries";
  const rows = z.object({entries: z.array(z.object({card: z.object({id: z.uuid(), etag: z.string()})}))});
  await withStagedPlatformEntry(workspace, name, async (id) => {
    const client = clientFor(FIXTURE_KEY.operator);
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_FLEET_LIBRARIES_PATH);
    const row = page.getByRole("row").filter({hasText: name});
    await row.getByRole("button", {name: DELETE, exact: true}).click();
    const dialog = page.getByRole("alertdialog", {name: DELETE_CONFIRM_TITLE});
    await expect(dialog).toBeVisible();
    const entry = rows.parse(await client.get<unknown>(path)).entries.find((item) => item.card.id === id);
    if (!entry) throw new Error("Staged entry disappeared before the deletion race");
    await client.patch(`${path}/${id}`, {description: "Changed after confirmation opened"}, {[SETUP_HEADERS.match]: entry.card.etag});
    await dialog.getByRole("button", {name: DELETE, exact: true}).click();
    await expect(dialog.getByRole("alert")).toBeVisible();
    await expect(dialog.getByRole("alert")).not.toBeEmpty();
    await expect(dialog).toBeVisible();
    await expect(row).toBeVisible();
    expect(rows.parse(await client.get<unknown>(path)).entries.some((item) => item.card.id === id)).toBe(true);
  });
});

test("an operator explicitly publishes and withdraws a staged native recipe", async ({page}) => {
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.operator);
  const name = `platform-chat-${crypto.randomUUID().slice(0, 8)}`;
  await withStagedPlatformEntry(workspace, name, async (libraryId) => {
    const client = clientFor(FIXTURE_KEY.operator);
    const gallery = `/v1/workspaces/${workspace}/fleet-libraries?limit=100`;
    const visible = async () => (await client.get<{items: {id: string}[]}>(gallery)).items.some((item) => item.id === libraryId);
    expect(await visible()).toBe(false);
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_FLEET_LIBRARIES_PATH);
    const row = page.getByRole("row").filter({hasText: name});
    await expect(row).toContainText("Draft");
    await row.getByRole("button", {name: PUBLISH, exact: true}).click();
    await expect(row).toContainText("Published");
    expect(await visible()).toBe(true);
    await row.getByRole("button", {name: EDIT, exact: true}).click();
    const edit = page.getByRole("dialog", {name: EDIT_TITLE});
    await edit.getByLabel(EDIT_DESCRIPTION_LABEL, {exact: true}).fill("Curated native recipe");
    await edit.getByRole("button", {name: "Save", exact: true}).click();
    await expect(edit).not.toBeVisible();
    await row.getByRole("button", {name: UNPUBLISH, exact: true}).click();
    await expect(row).toContainText("Draft");
    expect(await visible()).toBe(false);
  });
});

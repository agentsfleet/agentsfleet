import * as crypto from "node:crypto";
import { expect, test, type Route } from "@playwright/test";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";
import { signUpAs } from "./fixtures/signup";
import { workspaceHref } from "./fixtures/nav";

const PASSWORD = "SignupFixture!2026-stable";
const WORKSPACE_CREATE_TIMEOUT_MS = 30_000;

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `workspace-create-${tag}+clerk_test@e2e.agentsfleet.net`;
}

function uniqueWorkspaceName(): string {
  return `ui-created-${crypto.randomBytes(3).toString("hex")}`;
}

const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.agentsfleet.net");

test.describe("workspace create", () => {
  test.skip(
    isProdApi,
    "workspace-create signup runs only against development/local because Clerk test signups are development-only",
  );
  test.setTimeout(90_000);

  let createdEmail: string | null = null;

  test.afterEach(async () => {
    if (!createdEmail) return;
    const userId = await findUserIdByEmail(createdEmail).catch(() => null);
    if (userId) await deleteUser(userId).catch((err: unknown) => {
        // Loud, not silent: a swallowed failure here is how users leak. The
        // global-teardown sweep is the backstop for anything that slips.
        console.error(`[e2e] fixture user cleanup failed for ${userId}:`, err);
      });
    createdEmail = null;
  });

  test("fresh signup creates a workspace from the switcher dialog", async ({ page }) => {
    const email = uniqueEmail();
    const workspaceName = uniqueWorkspaceName();
    createdEmail = email;

    // Fresh signup lands on its OWN default workspace (not the fixture user's),
    // so navigate using the workspace id the signup helper resolved for this
    // brand-new tenant. The dialog then creates a *second* workspace.
    const { workspaceId } = await signUpAs(page, email, PASSWORD, { requireWorkspaceSession: true });
    if (!workspaceId) throw new Error("signup did not resolve a default workspace id");
    await page.goto(workspaceHref(workspaceId, "fleets"));

    const switcher = page.getByTestId("workspace-switcher");
    await expect(switcher).toBeVisible();
    await switcher.click();
    await page.getByTestId("workspace-new").click();

    const dialog = page.getByRole("dialog", { name: "Create workspace" });
    await expect(dialog).toBeVisible();
    await dialog.getByLabel("Name").fill(workspaceName);
    await dialog.getByRole("button", { name: "Create workspace" }).click();
    await expect(dialog).toBeHidden({ timeout: WORKSPACE_CREATE_TIMEOUT_MS });

    await expect(switcher).toContainText(workspaceName, {
      timeout: WORKSPACE_CREATE_TIMEOUT_MS,
    });
    await switcher.click();
    await expect(page.getByRole("menuitem", { name: workspaceName })).toBeVisible();

    await page.getByTestId("workspace-new").click();
    const duplicateDialog = page.getByRole("dialog", {
      name: "Create workspace",
    });
    await duplicateDialog.getByLabel("Name").fill(workspaceName);
    let duplicateActionPosts = 0;
    page.on("request", (request) => {
      if (request.method() === "POST" && request.headers()["next-action"]) {
        duplicateActionPosts += 1;
      }
    });
    await duplicateDialog
      .getByRole("button", { name: "Create workspace" })
      .click();

    await expect(duplicateDialog.getByTestId("workspace-create-error"))
      .toContainText(/already exists.*refreshing the workspace list/i);
    expect(duplicateActionPosts).toBe(1);
    await duplicateDialog.getByRole("button", { name: "Cancel" }).click();
    await switcher.click();
    await expect(page.getByRole("menuitem", { name: workspaceName }))
      .toHaveCount(1);

    const committedName = uniqueWorkspaceName();
    await page.getByTestId("workspace-new").click();
    const uncertainDialog = page.getByRole("dialog", {
      name: "Create workspace",
    });
    await uncertainDialog.getByLabel("Name").fill(committedName);
    let uncertainActionPosts = 0;
    const loseCommittedResponse = async (route: Route) => {
      const request = route.request();
      const isCreateAction =
        request.method() === "POST" && Boolean(request.headers()["next-action"]);
      if (!isCreateAction) {
        await route.continue();
        return;
      }
      uncertainActionPosts += 1;
      if (uncertainActionPosts > 1) {
        await route.continue();
        return;
      }
      await route.fetch();
      await route.abort("connectionreset");
    };
    await page.route("**/*", loseCommittedResponse);
    await uncertainDialog
      .getByRole("button", { name: "Create workspace" })
      .click();

    await expect(uncertainDialog.getByTestId("workspace-create-error"))
      .toContainText(/refreshing the workspace list.*before retrying/i);
    // Losing the response also loses the created workspace identifier. The
    // refresh can expose it in the menu, but must not silently change routes.
    await expect(switcher).toContainText(workspaceName);
    expect(uncertainActionPosts).toBe(1);
    await page.unroute("**/*", loseCommittedResponse);
    await uncertainDialog.getByRole("button", { name: "Cancel" }).click();
    await switcher.click();
    await expect(page.getByRole("menuitem", { name: committedName }))
      .toHaveCount(1);
  });
});

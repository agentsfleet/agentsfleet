import { beforeEach, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
const { credential, browser, feature } = vi.hoisted(() => ({ credential: vi.fn(), browser: vi.fn(), feature: {enabled: true} }));
vi.mock("@/lib/auth/credential", () => ({ requireCredential: credential }));
vi.mock("@/components/domain/plugins/PluginWorkspace", () => ({ PluginWorkspace: browser }));
vi.mock("@/lib/plugin-ui", () => ({pluginsUiEnabled: () => feature.enabled}));
vi.mock("next/navigation", () => ({notFound: () => {throw new Error("not-found");}}));
import PluginPage from "./page";
beforeEach(() => { vi.resetAllMocks(); feature.enabled = true; credential.mockResolvedValue("session"); browser.mockReturnValue(<p>Recipes</p>); });
it("requires authentication before opening the native Plugin for this workspace", async () => {
  const markup = renderToStaticMarkup(await PluginPage({ params: Promise.resolve({ workspaceId: "ws_1" }) }));
  expect(markup).toContain("Plugins");
  expect(browser.mock.calls[0]?.[0]).toMatchObject({ workspace: "ws_1" });
  expect(credential).toHaveBeenCalledOnce();
});
it("preserves sign-in failures without rendering the Plugin", async () => {
  credential.mockRejectedValue(new Error("sign-in"));
  await expect(PluginPage({ params: Promise.resolve({ workspaceId: "ws_1" }) })).rejects.toThrow("sign-in");
  expect(browser).not.toHaveBeenCalled();
});

it("keeps the paused route from reading credentials or rendering the retained tree", async () => {
  feature.enabled = false;
  await expect(PluginPage({params: Promise.resolve({workspaceId: "ws_1"})})).rejects.toThrow("not-found");
  expect(credential).not.toHaveBeenCalled();expect(browser).not.toHaveBeenCalled();
});

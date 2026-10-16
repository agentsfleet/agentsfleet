import { beforeEach, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
const { credential, browser } = vi.hoisted(() => ({ credential: vi.fn(), browser: vi.fn() }));
vi.mock("@/lib/auth/credential", () => ({ requireCredential: credential }));
vi.mock("@/components/domain/chat/LibraryWorkspace", () => ({ LibraryWorkspace: browser }));
import LibraryPage from "./page";
beforeEach(() => { vi.resetAllMocks(); credential.mockResolvedValue("session"); browser.mockReturnValue(<p>Recipes</p>); });
it("requires authentication before opening the native Library for this workspace", async () => {
  const markup = renderToStaticMarkup(await LibraryPage({ params: Promise.resolve({ workspaceId: "ws_1" }) }));
  expect(markup).toContain("Fleet library");
  expect(browser.mock.calls[0]?.[0]).toMatchObject({ workspace: "ws_1" });
  expect(credential).toHaveBeenCalledOnce();
});
it("preserves sign-in failures without rendering the Library", async () => {
  credential.mockRejectedValue(new Error("sign-in"));
  await expect(LibraryPage({ params: Promise.resolve({ workspaceId: "ws_1" }) })).rejects.toThrow("sign-in");
  expect(browser).not.toHaveBeenCalled();
});

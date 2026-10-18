import React from "react";
import { Button } from "@agentsfleet/design-system";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import type { LibraryModel } from "@/lib/api/model-library-types";
import { CATALOGUE_MODEL } from "./helpers/catalogue-fixtures";

const { read, push, router } = vi.hoisted(() => {
  const push = vi.fn();
  return { read: vi.fn(), push, router: { push } };
});
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({ getModelLibraryAction: read }));
vi.mock("next/navigation", () => ({ useRouter: () => router }));
import { ModelCatalogueProvider, useModelCatalogue } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider";

function page(models: LibraryModel[], next_cursor: string | null = null) {
  return { ok: true as const, data: { version: "v", models, next_cursor, total: null } };
}
function Probe() {
  const state = useModelCatalogue();
  return <div>
    <span data-testid="status">{state.status}</span>
    <span data-testid="error">{state.error}</span>
    <span data-testid="models">{state.models.map((model) => `${model.provider}:${model.id}:${model.display_name}`).join(",")}</span>
    <span data-testid="cursors">{JSON.stringify([state.nextCursor, state.advancedNextCursor])}</span>
    <Button type="button" onClick={state.preload}>base</Button><Button type="button" onClick={state.preloadAdvanced}>advanced</Button>
    <Button type="button" onClick={() => state.loadMore()}>more base</Button><Button type="button" onClick={() => state.loadMore(true)}>more all</Button>
    <Button type="button" onClick={state.retry}>retry</Button>
  </div>;
}
function setup() { render(<ModelCatalogueProvider><Probe /></ModelCatalogueProvider>); }
async function click(name: string) { await act(async () => { screen.getByRole("button", { name }).click(); }); }
async function settled(status = "ready") { await waitFor(() => expect(screen.getByTestId("status").textContent).toBe(status)); }

beforeEach(() => vi.resetAllMocks());
afterEach(cleanup);

it("appends opaque pages, deduplicating by provider and model while replacing refreshed rows", async () => {
  const otherProvider = { ...CATALOGUE_MODEL, provider: "other" };
  read.mockResolvedValueOnce(page([CATALOGUE_MODEL], "opaque/+cursor="))
    .mockResolvedValueOnce(page([{ ...CATALOGUE_MODEL, display_name: "Updated" }, otherProvider]));
  setup(); await click("retry"); await click("more base");
  expect(read).not.toHaveBeenCalled();
  await click("base"); await settled();
  await click("more base"); await settled();
  expect(read.mock.calls).toEqual([[false, null], [false, "opaque/+cursor="]]);
  expect(screen.getByTestId("models").textContent).toBe("fireworks:glm-5.2:Updated,other:glm-5.2:GLM 5.2");
  expect(screen.getByTestId("cursors").textContent).toBe("[null,null]");
  await click("more all"); await click("base");
  expect(read).toHaveBeenCalledTimes(2);
});

it("keeps independent cursors for ordinary and explicitly requested advanced models", async () => {
  const advanced = { ...CATALOGUE_MODEL, id: "advanced", advanced: true };
  read.mockResolvedValueOnce(page([CATALOGUE_MODEL], "base-next"))
    .mockResolvedValueOnce(page([advanced], "advanced-next"))
    .mockResolvedValueOnce(page([])).mockResolvedValueOnce(page([{ ...advanced, id: "advanced-2" }]));
  setup(); await click("advanced"); await settled();
  await waitFor(() => expect(read).toHaveBeenCalledTimes(2));
  expect(screen.getByTestId("cursors").textContent).toBe('["base-next","advanced-next"]');
  await click("more all"); await settled();
  await click("more base");
  expect(read).toHaveBeenCalledTimes(3);
  await click("more all"); await settled();
  expect(read.mock.calls).toEqual([[false, null], [true, null], [false, "base-next"], [true, "advanced-next"]]);
  expect(screen.getByTestId("models").textContent).toContain("advanced-2");
});

it.each([false, true])("retains rows and retries the failed cursor (advanced: %s)", async (advanced) => {
  read.mockResolvedValueOnce(page([CATALOGUE_MODEL], advanced ? null : "next"));
  if (advanced) read.mockResolvedValueOnce(page([{ ...CATALOGUE_MODEL, id: "advanced", advanced: true }], "next"));
  read.mockResolvedValueOnce({ ok: false, status: 422, error: "Invalid catalogue evidence" })
    .mockResolvedValueOnce(page([{ ...CATALOGUE_MODEL, id: "last", advanced }]));
  setup(); await click(advanced ? "advanced" : "base"); await settled();
  await click(advanced ? "more all" : "more base"); await settled("error");
  expect(screen.getByTestId("error").textContent).toBe("Invalid catalogue evidence");
  expect(screen.getByTestId("models").textContent).toContain("GLM 5.2");
  await click("retry"); await settled();
  expect(read.mock.calls.slice(-2)).toEqual([[advanced, "next"], [advanced, "next"]]);
  expect(screen.getByTestId("models").textContent).toContain("last");
  expect(screen.getByTestId("error").textContent).toBe("");
});

it("retains rows on a rejected page and recovers using the same cursor", async () => {
  read.mockResolvedValueOnce(page([CATALOGUE_MODEL], "cursor"))
    .mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(page([]));
  setup(); await click("base"); await settled(); await click("more base"); await settled("error");
  expect(screen.getByTestId("models").textContent).toContain("GLM 5.2");
  expect(screen.getByTestId("error").textContent).toContain("Try again");
  await click("retry"); await settled();
  expect(read.mock.calls.slice(-2)).toEqual([[false, "cursor"], [false, "cursor"]]);
});

it("keeps the current rows when a page requires reauthentication", async () => {
  read.mockResolvedValueOnce(page([CATALOGUE_MODEL], "cursor"))
    .mockResolvedValueOnce({ ok: false, status: 401, error: "Expired" });
  setup(); await click("base"); await settled(); await click("more base");
  expect(push).toHaveBeenCalledWith("/sign-in");
  expect(screen.getByTestId("models").textContent).toContain("GLM 5.2");
  expect(screen.getByTestId("error").textContent).toBe("");
});

it("coalesces more-page gestures while a page is unfinished", async () => {
  const unfinished = Promise.withResolvers<ReturnType<typeof page>>();
  read.mockResolvedValueOnce(page([CATALOGUE_MODEL], "cursor")).mockReturnValueOnce(unfinished.promise);
  setup(); await click("base"); await settled(); await click("more base"); await click("more base");
  expect(read).toHaveBeenCalledTimes(2);
  expect(screen.getByTestId("models").textContent).toContain("GLM 5.2");
  await act(async () => { unfinished.resolve(page([])); }); await settled();
});

it("leaves a queued advanced request pending until the failed ordinary read is retried", async () => {
  read.mockResolvedValueOnce({ ok: false, status: 503, error: "Ordinary catalogue unavailable" })
    .mockResolvedValueOnce(page([CATALOGUE_MODEL]))
    .mockResolvedValueOnce(page([{ ...CATALOGUE_MODEL, id: "advanced", advanced: true }]));
  setup(); await click("advanced"); await settled("error");
  expect(read).toHaveBeenCalledTimes(1);
  expect(screen.getByTestId("error").textContent).toBe("Ordinary catalogue unavailable");
  await click("retry"); await settled();
  await waitFor(() => expect(read).toHaveBeenCalledTimes(3));
  expect(screen.getByTestId("models").textContent).toContain("advanced");
  expect(read.mock.calls).toEqual([[false, null], [false, null], [true, null]]);
});

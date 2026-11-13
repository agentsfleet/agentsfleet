import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { TenantModelEntry } from "@/lib/types";
import ModelsRegistryTable from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable";
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => ({isLoaded: true, isSignedIn: true, userId: "model-owner"})}));

const { list, created } = vi.hoisted(() => ({list: vi.fn(), created: {current: () => {}}}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  listModelEntriesAction: list, listSecretsAction: vi.fn(), getModelLibraryAction: vi.fn(),
  resetProviderAction: vi.fn(), setProviderSelfManagedAction: vi.fn(), deleteModelEntryAction: vi.fn(),
}));
// A create already in flight can finish while a page read is outstanding.
// Keep its completion boundary controllable without retesting the create form.
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog", () => ({
  default: ({onCreated}: {onCreated: () => void}) => { created.current = onCreated; return null; },
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/EditModelEntryDialog", () => ({default: () => null}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog", () => ({default: () => null}));

function entry(name: string, active = false): TenantModelEntry {
  return {id: name, model_id: name, display_name: null, secret_ref: "key", provider: "anthropic", kind: "provider_key",
    workspace_id: "ws-test", has_key: true, active, created_at: "1970-01-01T00:00:00.001Z"};
}
function page(models: TenantModelEntry[], next_cursor: string | null) {
  return {models, next_cursor, total: null, platform_default_available: false};
}
function show() {
  const rows = Array.from({length: 25}, (_, i) => entry(`initial-${i}`, i === 0));
  render(<TooltipProvider><ModelsRegistryTable workspaceId="ws-test" initialPage={page(rows, "next")} initialError={null} /></TooltipProvider>);
}
function pendingPage() {
  let resolve!: (result: unknown) => void;
  list.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  return (models: TenantModelEntry[], cursor: string | null = null) => resolve({ok: true, data: page(models, cursor)});
}
beforeEach(() => vi.resetAllMocks());
afterEach(cleanup);

describe("model registry pagination concurrency", () => {
  it("locks row mutations while the next page is outstanding and restores them afterwards", async () => {
    const finish = pendingPage(); show();
    fireEvent.click(screen.getByRole("button", {name: "Load more models"}));
    expect(screen.getByRole("button", {name: "Manage initial-0"})).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", {name: "Make active: initial-1"})).toHaveProperty("disabled", true);
    await act(async () => { finish([entry("fresh")]); });
    expect(await screen.findByTitle("fresh")).toBeTruthy();
    await waitFor(() => expect(screen.getByRole("button", {name: "Manage fresh"})).toHaveProperty("disabled", false));
  });

  it("discards an old page response after a newer first-page refresh", async () => {
    const finishOld = pendingPage();
    list.mockResolvedValueOnce({ok: true, data: page([entry("newest", true)], null)});
    show(); fireEvent.click(screen.getByRole("button", {name: "Load more models"}));
    await act(async () => { created.current(); });
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
    await act(async () => { finishOld([entry("stale-page")], "stale-cursor"); });
    expect(screen.queryByTitle("stale-page")).toBeNull();
    expect(screen.getByTitle("newest")).toBeTruthy();
    expect(screen.queryByRole("button", {name: "Load more models"})).toBeNull();
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
    expect(list).toHaveBeenNthCalledWith(1, "next");
    expect(list).toHaveBeenNthCalledWith(2);
  });

  it("appends every returned model without hiding a page behind client pagination", async () => {
    const finish = pendingPage(); show();
    fireEvent.click(screen.getByRole("button", {name: "Load more models"}));
    await act(async () => finish(Array.from({length: 30}, (_, i) => entry(`fresh-${i}`))));
    expect(screen.getByTitle("initial-0")).toBeTruthy();
    expect(screen.getByTitle("fresh-29")).toBeTruthy();
    expect(screen.getAllByRole("listitem")).toHaveLength(55);
  });
});


it.each(["success", "failure"])("ignores a superseded first-page refresh that ends in %s", async (outcome) => {
  const older = Promise.withResolvers<unknown>();
  list.mockReturnValueOnce(older.promise).mockResolvedValueOnce({ok: true, data: page([entry("newest", true)], null)});
  show();
  await act(async () => {created.current();});
  await act(async () => {created.current();});
  await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  await act(async () => {
    if (outcome === "success") older.resolve({ok: true, data: page([entry("stale")], "old")});
    else older.reject(new Error("old refresh failed"));
  });
  expect(screen.getByTitle("newest")).toBeTruthy();
  expect(screen.queryByTitle("stale")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("ignores a rejected next-page read after refresh has replaced its collection", async () => {
  const older = Promise.withResolvers<unknown>();
  list.mockReturnValueOnce(older.promise).mockResolvedValueOnce({ok: true, data: page([entry("newest", true)], null)});
  show(); fireEvent.click(screen.getByRole("button", {name: "Load more models"}));
  await act(async () => {created.current();});
  await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  await act(async () => {older.reject(new Error("old page failed"));});
  expect(screen.getByTitle("newest")).toBeTruthy();
  expect(screen.queryByRole("alert")).toBeNull();
});


it("does not claim the platform default is active while another registry page is unread", () => {
  render(<TooltipProvider><ModelsRegistryTable workspaceId="ws-test" initialPage={{...page([entry("inactive")], "next"), platform_default_available: true}} initialError={null} /></TooltipProvider>);
  expect(screen.queryByText("Active")).toBeNull();
});

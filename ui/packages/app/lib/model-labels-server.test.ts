// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest";

const { getModelLibrary } = vi.hoisted(() => ({ getModelLibrary: vi.fn() }));
vi.mock("@/lib/api/model_library", () => ({ getModelLibrary }));

import { modelLabelsCached } from "./model-labels-server";

const page = (models: { id: string; display_name: string }[], next_cursor: string | null) => ({ version: "v", models, total: null, next_cursor });
afterEach(() => { vi.clearAllMocks(); });

describe("the per-load label read", () => {
  it("walks the complete catalogue, advanced models included, and resolves a split dispute", async () => {
    getModelLibrary
      .mockResolvedValueOnce(page([{ id: "known", display_name: "Known" }, { id: "split", display_name: "Alpha" }], "next"))
      .mockResolvedValueOnce(page([{ id: "split", display_name: "Beta" }], null));
    expect(await modelLabelsCached("token-one")).toEqual({ known: "Known" });
    expect(getModelLibrary.mock.calls).toEqual([["token-one", true, null, 100], ["token-one", true, "next", 100]]);
  });

  it("propagates a failed read so the layout shows recorded identifiers", async () => {
    getModelLibrary.mockRejectedValueOnce(new Error("catalogue down"));
    await expect(modelLabelsCached("token-two")).rejects.toThrow("catalogue down");
  });

  it("refuses a catalogue that never ends instead of labelling from part of it", async () => {
    getModelLibrary.mockResolvedValue(page([{ id: "known", display_name: "Known" }], "again"));
    await expect(modelLabelsCached("token-three")).rejects.toThrow("page bound");
    expect(getModelLibrary).toHaveBeenCalledTimes(40);
  });
});

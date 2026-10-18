import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import { capturedOnSortChange, entry, registry, renderTable, renderTableWithLibrary, PLATFORM_LOCK } from "./helpers/models-registry-fixtures";

describe("Models registry names and rates", () => {
  it("default row renders the platform default's model, provider, context, and library rates", async () => {
    await renderTableWithLibrary(
      registry([], true, { provider: "anthropic", model: "claude-sonnet-5", display_name: "Sonnet 5", context_cap_tokens: 200000 }),
    );

    const rows = screen.getAllByRole("row");
    const defaultRow = within(rows[1]!);
    expect(defaultRow.getByLabelText(PLATFORM_LOCK)).toBeTruthy();
    expect(defaultRow.getByTitle("claude-sonnet-5")).toBeTruthy();
    expect(defaultRow.getByText("Anthropic")).toBeTruthy();
    expect(defaultRow.getByText("200,000")).toBeTruthy();
    await waitFor(() => expect(defaultRow.getByText("3.00 / 0.30 / 15.00")).toBeTruthy());
  });

  it("default row renders server-provided rates when the public catalogue is unavailable", async () => {
    await renderTable(
      registry([], true, {
        provider: "anthropic",
        model: "claude-sonnet-5", display_name: "Sonnet 5",
        context_cap_tokens: 200000,
        input_nanos_per_mtok: 3_000_000_000,
        cached_input_nanos_per_mtok: 300_000_000,
        output_nanos_per_mtok: 15_000_000_000,
      }),
    );

    const rows = screen.getAllByRole("row");
    const defaultRow = within(rows[1]!);
    expect(defaultRow.getByText("200,000")).toBeTruthy();
    expect(defaultRow.getByText("3.00 / 0.30 / 15.00")).toBeTruthy();
  });

  it("default row degrades to '—' when no platform default identity rides the list", async () => {
    // No active entry, so the platform row still renders (it is what would run)
    // — but with no identity to show it must degrade rather than invent one.
    await renderTable(registry([], false));
    const rows = screen.getAllByRole("row");
    const defaultRow = within(rows[1]!);
    // Context and price are separate columns now, so an absent identity
    // degrades in both rather than in one shared cell.
    expect(defaultRow.getAllByText("—")).toHaveLength(2);
    expect(screen.getByText("No default is configured.")).toBeTruthy();
  });

  it("entry rows price from the library when known and say who bills otherwise", async () => {
    await renderTableWithLibrary(
      registry([
        entry({ id: "e1", model_id: "claude-sonnet-5", provider: "anthropic", context_cap_tokens: 200000 }),
        entry({ id: "e2", model_id: "local-model", provider: "openai-compatible", base_url: "https://vllm.corp/v1", context_cap_tokens: 32000 }),
      ]),
    );

    await waitFor(() => expect(screen.getByText("3.00 / 0.30 / 15.00")).toBeTruthy());
    const rows = screen.getAllByRole("row");
    // Row order: header, Default, sonnet (priced), local (unpriced).
    const localRow = within(rows[3]!);
    expect(localRow.getByText("32,000")).toBeTruthy();
    // A tenant entry is self-managed by definition, so an unpriced row is "not
    // applicable", not a lookup miss — and a price here would imply agentsfleet
    // is charging it when the tenant's own provider bills them directly.
    expect(localRow.getByText("Billed by provider")).toBeTruthy();
    expect(localRow.queryByText("Rates unavailable")).toBeNull();
  });

  it("entry rows render server-provided rates without depending on the public catalogue", async () => {
    await renderTable(
      registry([
        entry({
          id: "e1",
          model_id: "claude-sonnet-5",
          provider: "anthropic",
          context_cap_tokens: 200000,
          input_nanos_per_mtok: 3_000_000_000,
          cached_input_nanos_per_mtok: 300_000_000,
          output_nanos_per_mtok: 15_000_000_000,
        }),
      ]),
    );

    expect(screen.getByText("200,000")).toBeTruthy();
    expect(screen.getByText("3.00 / 0.30 / 15.00")).toBeTruthy();
  });

  it("shows the 'no key · local' badge on an entry with no key, and the endpoint host in the Provider cell", async () => {
    await renderTable(
      registry([entry({ provider: "openai-compatible", base_url: "https://vllm.corp/v1", has_key: false })]),
    );
    expect(screen.getByText("no key · local")).toBeTruthy();
    expect(screen.getByText("https://vllm.corp/v1")).toBeTruthy();
  });

  it("shows Unknown in the Provider cell when the entry has no provider", async () => {
    await renderTable(registry([entry({ provider: undefined })]));
    expect(screen.getByText("Unknown")).toBeTruthy();
  });

  it("ignores an onSortChange call for a key outside the sortable column set", async () => {
    await renderTable(registry([entry({})]));
    const rowsBefore = screen.getAllByRole("row").length;

    capturedOnSortChange?.("status");

    expect(screen.getAllByRole("row").length).toBe(rowsBefore);
    expect(screen.getByRole("columnheader", { name: "Model" }).getAttribute("aria-sort")).toBe("none");
  });

  it("compareEntries orders models as the library does and providers by name, a missing one first", async () => {
    const { compareEntries } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/components/registry-view");
    const big = entry({ model_id: "openai/gpt-oss-120b", provider: "openai" });
    const small = entry({ model_id: "openai/gpt-oss-20b", provider: "anthropic" });
    expect([big, small].sort((a, b) => compareEntries(a, b, "model")).map((e) => e.model_id)).toEqual(["openai/gpt-oss-20b", "openai/gpt-oss-120b"]);
    expect(compareEntries(big, small, "provider")).toBeGreaterThan(0);
    expect(compareEntries(entry({ provider: undefined }), small, "provider")).toBeLessThan(0);
    expect(compareEntries(small, entry({ provider: undefined }), "provider")).toBeGreaterThan(0);
    // By the name the Provider cell shows: OpenAI before Tencent Hunyuan,
    // though by slug `hunyuan` would sort first.
    expect(compareEntries(entry({ provider: "hunyuan" }), entry({ provider: "openai" }), "provider")).toBeGreaterThan(0);
  });

  it("computeNextSort accepts Context and Rates, the columns the model library sorts", async () => {
    const { computeNextSort } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/registry-view"
    );
    expect(computeNextSort(null, "context")).toEqual({ key: "context", dir: "ascending" });
    expect(computeNextSort(null, "rates")).toEqual({ key: "rates", dir: "ascending" });
  });

  it("compareEntries orders context and rates by the figure the row shows, an absent one first", async () => {
    const { compareEntries } = await import("../app/(dashboard)/w/[workspaceId]/settings/models/components/registry-view");
    const wide = entry({ context_cap_tokens: 1_000_000 });
    const narrow = entry({ context_cap_tokens: 128_000 });
    expect(compareEntries(wide, narrow, "context")).toBeGreaterThan(0);
    expect(compareEntries(entry({ context_cap_tokens: undefined }), narrow, "context")).toBeLessThan(0);
    expect(compareEntries(narrow, entry({ context_cap_tokens: undefined }), "context")).toBeGreaterThan(0);

    // An entry priced by its own fields uses them; one without uses the
    // library's rate for its (provider, model), exactly as the Rates cell does.
    const priced = entry({
      provider: "openai",
      model_id: "own",
      input_nanos_per_mtok: 5,
      cached_input_nanos_per_mtok: 1,
      output_nanos_per_mtok: 9,
    });
    const library = entry({ provider: "openai", model_id: "listed" });
    const unpriced = entry({ provider: "openai", model_id: "unlisted" });
    const models = [
      {
        id: "listed",
        display_name: "Listed",
        advanced: false,
        provider: "openai",
        context_cap_tokens: 1,
        input_nanos_per_mtok: 3,
        cached_input_nanos_per_mtok: 1,
        output_nanos_per_mtok: 4,
      },
    ];
    expect(compareEntries(priced, library, "rates", models)).toBeGreaterThan(0);
    expect(compareEntries(unpriced, library, "rates", models)).toBeLessThan(0);
    expect(compareEntries(library, unpriced, "rates")).toBe(0);
  });

  it("groups the context column in threes, as the model library prints it", async () => {
    await renderTable(
      registry([
        entry({ id: "e1", model_id: "m1", context_cap_tokens: 200000 }),
        entry({ id: "e2", model_id: "m2", context_cap_tokens: 500 }),
      ]),
    );
    expect(screen.getByText("200,000")).toBeTruthy();
    expect(screen.getByText("500")).toBeTruthy();
  });

  it("renders an explicit 0-token cap as '0', not '—' (nullish guard, not falsy)", async () => {
    await renderTable(registry([entry({ id: "e1", model_id: "m1", context_cap_tokens: 0 })]));
    expect(screen.getByText("0")).toBeTruthy();
  });

  it("renders a dash for absent context and names who bills", async () => {
    await renderTable(registry([entry({ id: "e1", model_id: "m1", context_cap_tokens: undefined })]));
    const rows = screen.getAllByRole("row");
    const contextCell = within(rows[2]!);
    expect(contextCell.getByText("—")).toBeTruthy();
    expect(contextCell.getByText("Billed by provider")).toBeTruthy();
  });

});

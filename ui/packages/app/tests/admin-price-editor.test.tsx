import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { ActionForm, Button, Form } from "@agentsfleet/design-system";
import { ModelSettingsFields } from "@/app/(dashboard)/admin/models/components/ModelSettingsFields";
import { modelSettings, settingsSchema, valuesFromModel, type ModelSettingsValues } from "@/app/(dashboard)/admin/models/components/model-settings";
import { ADMIN_MODEL, PRICE, SOURCE_URL, VERIFIED_AT } from "./helpers/catalogue-fixtures";

afterEach(cleanup);
const DEFAULTS = valuesFromModel(ADMIN_MODEL);
function Harness({ onSave, defaults = DEFAULTS }: { onSave: (value: unknown) => void; defaults?: ModelSettingsValues }) {
  const form = useForm<ModelSettingsValues>({ defaultValues: defaults, resolver: zodResolver(settingsSchema) });
  return <Form {...form}><ActionForm noValidate onSubmit={(event) => void form.handleSubmit((values) => onSave(modelSettings(values)))(event)}>
    <ModelSettingsFields /><Button type="submit">Save</Button>
  </ActionForm></Form>;
}
function change(label: string, value: string, index = 0) {
  const field = screen.getAllByLabelText(label)[index];
  if (!field) throw new Error(`Missing field ${label}`);
  fireEvent.change(field, { target: { value } });
}

describe("admin price array editor", () => {
  it("submits exact cache writes, evidence, metadata and inference settings", async () => {
    const save = vi.fn();
    render(<Harness onSave={save} />);
    change("Catalogue display name", "Label from catalogue");
    change("Cache write ($/1M)", "0");
    change("Cache write, 5-minute ($/1M)", "0.0625");
    change("Cache write, 1-hour ($/1M)", "9223372036.854775807");
    change("Conditions", "Global pricing");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    expect(save.mock.calls[0]?.[0]).toMatchObject({
      metadata: { ...ADMIN_MODEL.metadata, display_name: "Label from catalogue" },
      inference: ADMIN_MODEL.inference,
      prices: [{ ...PRICE, cache_write_nanos_per_mtok: "0", cache_write_5m_nanos_per_mtok: "62500000", cache_write_1h_nanos_per_mtok: "9223372036854775807", conditions: "Global pricing" }],
    });
  });
  it("adds and removes complete tiers while protecting the final price", async () => {
    const save = vi.fn();
    render(<Harness onSave={save} />);
    expect(screen.getByRole("button", { name: "Remove price 1" }).hasAttribute("disabled")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Add price" }));
    fireEvent.click(screen.getByRole("button", { name: "Price 2" }));
    for (const [label, value] of [["Prompt tokens above", "272000"], ["Effective from", VERIFIED_AT], ["Price source URL", SOURCE_URL], ["Price verified at", VERIFIED_AT], ["Input ($/1M)", "4"], ["Cached ($/1M)", "0.1"], ["Output ($/1M)", "20"]]) change(label!, value!, 1);
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    expect(save.mock.calls[0]?.[0].prices[1]).toMatchObject({ prompt_tokens_above: 272000, input_nanos_per_mtok: "4000000000", cache_write_nanos_per_mtok: null });
    fireEvent.click(screen.getByRole("button", { name: "Remove price 2" }));
    expect(screen.queryByRole("button", { name: "Price 2" })).toBeNull();
  });
  it("shows a missing base-tier refusal and retains the operator's input", async () => {
    const save = vi.fn();
    render(<Harness onSave={save} />);
    change("Prompt tokens above", "1");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText(/Include a base tier/)).toBeTruthy();
    expect((screen.getByLabelText("Prompt tokens above") as HTMLInputElement).value).toBe("1");
    expect(save).not.toHaveBeenCalled();
  });
  it("shows field-level precision and reversed-date refusals", async () => {
    const save = vi.fn();
    render(<Harness onSave={save} />);
    change("Input ($/1M)", "0.0000000001");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText(/at most nine fractional digits/)).toBeTruthy();
    change("Input ($/1M)", "0.55");
    change("Effective until", "2026-10-17T00:00:00Z");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Effective until must be after effective from")).toBeTruthy();
    expect(save).not.toHaveBeenCalled();
  });
  it("discloses optional inference semantics and hides its fields when left unchanged", () => {
    render(<Harness onSave={vi.fn()} />);
    fireEvent.click(screen.getByRole("switch", { name: "Write inference settings" }));
    expect(screen.queryByLabelText("Inference API")).toBeNull();
    fireEvent.click(screen.getByRole("switch", { name: "Advanced model" }));
    expect(screen.getByRole("switch", { name: "Advanced model" }).getAttribute("aria-checked")).toBe("true");
  });
});

it("summarizes an overlapping tier even while that price row is collapsed", async () => {
  const save = vi.fn();
  render(<Harness onSave={save} defaults={{ ...DEFAULTS, prices: [DEFAULTS.prices[0]!, DEFAULTS.prices[0]!] }} />);
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByText(/Price 2: Effective from: Date ranges for the same prompt threshold must not overlap/)).toBeTruthy();
  expect(save).not.toHaveBeenCalled();
});
it("caps the editor at 64 prices and permits another only after removing a row", () => {
  const prices = Array.from({ length: 64 }, (_, index) => ({ ...DEFAULTS.prices[0]!, prompt_tokens_above: String(index) }));
  render(<Harness onSave={vi.fn()} defaults={{ ...DEFAULTS, prices }} />);
  expect(screen.getByRole("button", { name: "Add price" })).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", { name: "Remove price 1" }));
  expect(screen.getByRole("button", { name: "Add price" })).toHaveProperty("disabled", false);
});

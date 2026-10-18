import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { ADMIN_MODEL, PRICE, SOURCE_URL, VERIFIED_AT } from "./helpers/catalogue-fixtures";

const { create, update } = vi.hoisted(() => ({ create: vi.fn(), update: vi.fn() }));
vi.mock("@/app/(dashboard)/admin/models/actions", () => ({ createAdminModelAction: create, updateAdminModelAction: update }));
import AddModelDialog from "@/app/(dashboard)/admin/models/components/AddModelDialog";
import EditModelDialog from "@/app/(dashboard)/admin/models/components/EditModelDialog";

function change(label: string, value: string) { fireEvent.change(screen.getByLabelText(label), { target: { value } }); }
function setup(edit: boolean) {
  const saved = vi.fn();
  const close = vi.fn();
  render(<TooltipProvider>{edit
    ? <EditModelDialog model={ADMIN_MODEL} onOpenChange={close} onUpdated={saved} />
    : <AddModelDialog defaultOpen onCreated={saved} />}</TooltipProvider>);
  if (!edit) for (const [label, value] of [
    ["Provider", ADMIN_MODEL.provider], ["Model", ADMIN_MODEL.model_id],
    ["Catalogue display name", ADMIN_MODEL.metadata.display_name], ["Context window (tokens)", String(ADMIN_MODEL.metadata.context_cap_tokens)],
    ["Context source URL", SOURCE_URL], ["Context verified at", VERIFIED_AT],
    ["Effective from", VERIFIED_AT], ["Price source URL", SOURCE_URL], ["Price verified at", VERIFIED_AT],
    ["Input ($/1M)", "0.55"], ["Cached ($/1M)", "0.14"], ["Output ($/1M)", "2.19"],
  ]) change(label!, value!);
  return { saved, close, submit: () => fireEvent.submit(screen.getByRole("dialog").querySelector("form")!) };
}

beforeEach(() => vi.resetAllMocks());
afterEach(cleanup);

it.each([false, true])("keeps entered prices after a server schedule refusal (edit: %s)", async (edit) => {
  const action = edit ? update : create;
  const refusal = "Price list needs 1–64 entries, a base tier and non-overlapping dates";
  action.mockResolvedValueOnce({ ok: false, error: refusal });
  const { saved, submit } = setup(edit);
  change("Cache write, 5-minute ($/1M)", "0.000000001");
  submit();
  await waitFor(() => expect(within(screen.getByRole("dialog")).getByText(new RegExp(refusal))).toBeTruthy());
  expect(screen.getByLabelText("Cache write, 5-minute ($/1M)")).toHaveProperty("value", "0.000000001");
  expect(saved).not.toHaveBeenCalled();
});
it.each([false, true])("retains exact inputs on a rejected action round trip, then saves on retry (edit: %s)", async (edit) => {
  const action = edit ? update : create;
  action.mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce({ ok: true, data: edit ? { id: ADMIN_MODEL.id, updated: true } : ADMIN_MODEL });
  const { saved, submit } = setup(edit);
  change("Input ($/1M)", "9223372036.854775807");
  submit();
  await screen.findByText(/Your entries are retained; try again/);
  expect(screen.getByLabelText("Input ($/1M)")).toHaveProperty("value", "9223372036.854775807");
  expect(saved).not.toHaveBeenCalled();
  submit();
  await waitFor(() => expect(saved).toHaveBeenCalledOnce());
  const body = action.mock.calls[1]?.[edit ? 1 : 0];
  expect(body.prices[0].input_nanos_per_mtok).toBe("9223372036854775807");
});
it("preserves inference when omitted and updates the returned row with the exact new schedule", async () => {
  update.mockResolvedValueOnce({ ok: true, data: { id: ADMIN_MODEL.id, updated: true } });
  const { saved, close, submit } = setup(true);
  fireEvent.click(screen.getByRole("switch", { name: "Write inference settings" }));
  change("Cache write ($/1M)", "0");
  change("Cache write, 1-hour ($/1M)", "9223372036.854775807");
  submit();
  const prices = [{ ...PRICE, cache_write_nanos_per_mtok: "0", cache_write_1h_nanos_per_mtok: "9223372036854775807" }];
  await waitFor(() => expect(saved).toHaveBeenCalledWith({ ...ADMIN_MODEL, prices }));
  expect(update).toHaveBeenCalledWith(ADMIN_MODEL.id, { metadata: ADMIN_MODEL.metadata, prices });
  expect(close).toHaveBeenCalledWith(false);
});

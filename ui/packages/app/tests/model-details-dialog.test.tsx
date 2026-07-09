import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import ModelDetailsDialog from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog";
import type { TenantModelEntry } from "@/lib/types";

// The dialog renders real design-system primitives — Time (format="relative")
// and IconAction-free header Badge — both of which sit under Radix. A relative
// Time renders a Radix Tooltip, which needs a <TooltipProvider> ancestor
// (mounted at the dashboard layout root in production, not in tests).
function renderDialog(target: TenantModelEntry) {
  return render(
    <TooltipProvider>
      <ModelDetailsDialog target={target} onOpenChange={() => {}} />
    </TooltipProvider>,
  );
}

/** Two hours in epoch-ms — old enough that the relative label is a stable "… ago". */
const TWO_HOURS_MS = 2 * 60 * 60 * 1_000;

// Base fixture: a vault-backed custom endpoint entry. `secret_ref` is the vault
// key reference (e.g. "pioneer") — the value the old "Name" row duplicated
// against Provider. `created_at` is epoch-ms two hours in the past so the
// relative label is a stable "… ago".
function makeEntry(overrides: Partial<TenantModelEntry> = {}): TenantModelEntry {
  return {
    id: "mdl_1",
    model_id: "gpt-4o",
    secret_ref: "pioneer",
    provider: "openai",
    kind: "provider_key",
    base_url: "https://api.example.com/v1",
    has_key: true,
    active: true,
    created_at: Date.now() - TWO_HOURS_MS,
    ...overrides,
  };
}

// Description terms in document order. The dialog renders each <dt> inside the
// DialogContent portal (appended to document.body), so query the document, not
// the render container.
function termTexts(): string[] {
  return Array.from(document.querySelectorAll("dt")).map((dt) => dt.textContent?.trim() ?? "");
}

afterEach(() => cleanup());

describe("ModelDetailsDialog", () => {
  it("test_details_row_order_no_kind", () => {
    renderDialog(makeEntry());

    // Kind and Has key are gone entirely — neither as a term nor anywhere else.
    expect(screen.queryByText("Kind")).toBeNull();
    expect(screen.queryByText(/has key/i)).toBeNull();

    // With base_url set, the description terms are exactly these, in this order.
    expect(termTexts()).toEqual(["Provider", "Model", "Secret ref", "Endpoint"]);
  });

  it("test_details_row_order_no_kind_without_base_url", () => {
    // Endpoint appears only when base_url is set; drop it and the term vanishes.
    renderDialog(makeEntry({ base_url: undefined }));
    expect(termTexts()).toEqual(["Provider", "Model", "Secret ref"]);
  });

  it("test_secret_ref_row_label", () => {
    renderDialog(makeEntry({ secret_ref: "pioneer" }));

    const term = screen.getByText("Secret ref");
    expect(term.tagName).toBe("DT");
    // The value sits in the sibling <dd> of the same row group.
    const value = term.parentElement?.querySelector("dd");
    expect(value?.textContent).toBe("pioneer");
  });

  it("test_vault_badge_reflects_has_key", () => {
    renderDialog(makeEntry({ has_key: true }));
    expect(screen.getByText("In vault")).toBeTruthy();
    expect(screen.queryByText("Keyless endpoint")).toBeNull();

    cleanup();

    renderDialog(makeEntry({ has_key: false }));
    expect(screen.getByText("Keyless endpoint")).toBeTruthy();
    expect(screen.queryByText("In vault")).toBeNull();
  });

  it("test_added_time_relative_in_header", () => {
    renderDialog(makeEntry());

    // The header carries an "Added <relative>" — a real <time> element with a
    // canonical datetime attr and a relative visible label.
    expect(screen.getByText(/added/i)).toBeTruthy();
    const timeEl = document.querySelector("time");
    expect(timeEl).not.toBeNull();
    expect(timeEl?.getAttribute("datetime")).toBeTruthy();
    expect(timeEl?.textContent).toMatch(/ago|just now|in \d/);

    // Creation is header context now, not a description row.
    expect(termTexts()).not.toContain("Created");
    expect(screen.queryByText("Created")).toBeNull();
  });

  it("test_time_invalid_timestamp_dash", () => {
    // A NaN created_at must degrade to "—" via Time's own guard — no thrown
    // RangeError tearing down the dialog subtree.
    expect(() => renderDialog(makeEntry({ created_at: Number.NaN }))).not.toThrow();
    expect(screen.getByText("—")).toBeTruthy();
  });
});

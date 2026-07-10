import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen, cleanup } from "@testing-library/react";
import { IconAction } from "./IconAction";
function renderAction(node: React.ReactElement) {
  return render(node);
}

const Glyph = () => <svg data-testid="glyph" aria-hidden="true" />;

describe("IconAction", () => {
  it("test_icon_action_accessible_name", () => {
    renderAction(
      <IconAction label="Cordon">
        <Glyph />
      </IconAction>,
    );

    // The single `label` prop is the button's accessible name — an icon alone
    // is not a name. aria-label survives even if the floating tooltip never
    // opens.
    const button = screen.getByRole("button", { name: "Cordon" });
    expect(button).toBeInTheDocument();
    expect(button.getAttribute("aria-label")).toBe("Cordon");

    // Size is fixed to `icon-sm` (h-6 w-6) — there is no size prop, so every
    // row action is uniform. Prove it renders icon-sm and NOT the larger
    // `icon` (h-9) or default (h-10) sizings.
    const cls = button.className;
    expect(cls).toContain("h-6");
    expect(cls).toContain("w-6");
    expect(cls).not.toContain("h-9");
    expect(cls).not.toContain("h-10");
  });

  it("test_icon_action_tooltip_shows_label", async () => {
    renderAction(
      <IconAction label="Cordon">
        <Glyph />
      </IconAction>,
    );

    // Radix opens the tooltip immediately on focus (the hover delay does not
    // apply to keyboard focus) and portals the content out of the trigger.
    // TooltipContent carries role="tooltip"; its body is the same `label`.
    const button = screen.getByRole("button", { name: "Cordon" });
    fireEvent.focus(button);

    const tooltip = await screen.findByRole("tooltip");
    expect(tooltip).toHaveTextContent("Cordon");
  });

  it("test_icon_action_variant_and_passthrough", () => {
    const onClick = vi.fn();
    renderAction(
      <IconAction label="Revoke" variant="destructive" onClick={onClick}>
        <Glyph />
      </IconAction>,
    );

    // variant reaches the underlying Button's class map.
    const button = screen.getByRole("button", { name: "Revoke" });
    expect(button.className).toContain("bg-destructive");

    // onClick passes through and fires on an enabled action.
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);

    // disabled passes through and blocks the click (React does not dispatch a
    // click on a disabled button). Fresh render so only one "Revoke" is mounted.
    cleanup();
    const onDisabledClick = vi.fn();
    renderAction(
      <IconAction label="Revoke" variant="destructive" onClick={onDisabledClick} disabled>
        <Glyph />
      </IconAction>,
    );
    const disabledButton = screen.getByRole("button", { name: "Revoke" });
    expect(disabledButton).toBeDisabled();
    expect(disabledButton.className).toContain("pointer-events-none");
    expect(disabledButton.parentElement?.tagName).toBe("SPAN");
    fireEvent.click(disabledButton);
    expect(onDisabledClick).not.toHaveBeenCalled();
  });
});

// Compile-time proof (never executed): the type rejects a nameless IconAction
// and rejects an attempt to override the fixed `icon-sm` size. vitest strips
// types, so this is inert at runtime; `tsc` verifies the @ts-expect-error lines.
function __typeContracts() {
  return (
    <>
      {/* @ts-expect-error label is required — an icon-only action must be named. */}
      <IconAction>
        <Glyph />
      </IconAction>
      {/* @ts-expect-error size is fixed to icon-sm and is not a prop. */}
      <IconAction label="X" size="icon">
        <Glyph />
      </IconAction>
    </>
  );
}
// Reference the contract so `tsc --noEmit` doesn't flag it as unused (TS6133)
// while still never invoking it at runtime.
void __typeContracts;

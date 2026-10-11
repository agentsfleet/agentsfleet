import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { ActionForm } from "./ActionForm";

describe("ActionForm", () => {
  it("renders a labelled native form that merges the caller className", () => {
    render(
      <ActionForm aria-label="Model setup" className="text-sm">
        <button type="submit">Save</button>
      </ActionForm>,
    );

    expect(screen.getByRole("form", { name: "Model setup" })).toHaveClass("text-sm");
    expect(screen.getByRole("button", { name: "Save" })).toBeTruthy();
  });
});

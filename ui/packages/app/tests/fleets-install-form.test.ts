import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetCommonMocks } from "./helpers/dashboard-mocks";

// InstallFleetForm is now a pure paste INPUT: it validates the SKILL.md (and
// optional TRIGGER.md) frontmatter client-side, then hands the validated
// markdown to the install states via `onSubmit`. It does NOT post or route —
// create runs inline in the states. These tests pin the validation gate + the
// callback shape.
vi.mock("next/navigation", async () => (await import("./helpers/dashboard-mocks")).nextNavigationMock());

import InstallFleetForm from "../app/(dashboard)/fleets/new/InstallFleetForm";

const FIXTURE_TRIGGER =
  "---\nname: platform-ops\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools:\n    - agentmail\n  budget:\n    daily_dollars: 1.0\n---\n";
const FIXTURE_SKILL =
  "---\nname: platform-ops\ndescription: Automates platform checks\nversion: 0.1.0\n---\n# Platform Ops\n";

let onSubmit: ReturnType<typeof vi.fn<(sourceMarkdown: string, triggerMarkdown?: string) => void>>;
let onBack: ReturnType<typeof vi.fn<() => void>>;

function renderForm() {
  onSubmit = vi.fn<(sourceMarkdown: string, triggerMarkdown?: string) => void>();
  onBack = vi.fn<() => void>();
  return render(React.createElement(InstallFleetForm, { onSubmit, onBack }));
}

beforeEach(() => {
  vi.clearAllMocks();
  resetCommonMocks();
});
afterEach(() => cleanup());

describe("InstallFleetForm — paste input", () => {
  it("blank TRIGGER.md submits SKILL.md only (server defaults the wake)", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    expect(screen.getByText(/What is SKILL\.md/i)).toBeTruthy();
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /create fleet/i }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    const [skill, trigger] = onSubmit.mock.calls[0]!;
    expect(skill).toContain("Platform Ops");
    expect(trigger).toBeUndefined();
  });

  it("a filled TRIGGER.md is passed alongside the SKILL.md", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    const skillField = screen.getByLabelText(/SKILL\.md body/i);
    const triggerField = screen.getByLabelText(/TRIGGER\.md body/i);
    expect(skillField.compareDocumentPosition(triggerField) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await user.type(triggerField, FIXTURE_TRIGGER);
    await user.type(skillField, FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /create fleet/i }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    const [skill, trigger] = onSubmit.mock.calls[0]!;
    expect(skill).toContain("Platform Ops");
    expect(trigger).toContain("x-agentsfleet:");
  });

  it("empty SKILL.md blocks submit with the required-field error", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    await user.type(screen.getByLabelText(/TRIGGER\.md body/i), FIXTURE_TRIGGER);
    await user.click(screen.getByRole("button", { name: /create fleet/i }));
    await waitFor(() => expect(screen.getByText(/SKILL\.md body is required/i)).toBeTruthy());
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("malformed SKILL.md (no frontmatter markers) blocks submit", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    await user.type(screen.getByLabelText(/SKILL\.md body/i), "# missing frontmatter");
    await user.click(screen.getByRole("button", { name: /create fleet/i }));
    await waitFor(() => expect(screen.getByText(/SKILL\.md needs frontmatter/i)).toBeTruthy());
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("SKILL.md missing a required frontmatter field blocks submit", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    await user.type(
      screen.getByLabelText(/SKILL\.md body/i),
      "---\nname: platform-ops\ndescription: Automates platform checks\n---\n# Platform Ops\n",
    );
    await user.click(screen.getByRole("button", { name: /create fleet/i }));
    await waitFor(() => expect(screen.getByText(/SKILL\.md frontmatter needs version:/i)).toBeTruthy());
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("SKILL.md without a closing frontmatter marker blocks submit", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    await user.type(
      screen.getByLabelText(/SKILL\.md body/i),
      "---\nname: platform-ops\ndescription: d\nversion: 0.1.0\n# Platform Ops\n",
    );
    await user.click(screen.getByRole("button", { name: /create fleet/i }));
    await waitFor(() => expect(screen.getByText(/SKILL\.md needs frontmatter/i)).toBeTruthy());
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("malformed TRIGGER.md (missing x-agentsfleet) blocks submit", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    await user.type(screen.getByLabelText(/TRIGGER\.md body/i), "---\nname: platform-ops\n---\n");
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /create fleet/i }));
    await waitFor(() =>
      expect(screen.getByText(/TRIGGER\.md frontmatter needs x-agentsfleet:/i)).toBeTruthy(),
    );
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("TRIGGER.md without frontmatter markers blocks submit", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    await user.type(screen.getByLabelText(/TRIGGER\.md body/i), "name: platform-ops");
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /create fleet/i }));
    await waitFor(() => expect(screen.getByText(/TRIGGER\.md needs frontmatter/i)).toBeTruthy());
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("TRIGGER.md without a top-level name blocks submit", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    await user.type(
      screen.getByLabelText(/TRIGGER\.md body/i),
      "---\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools:\n    - agentmail\n  budget:\n    daily_dollars: 1.0\n---\n",
    );
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /create fleet/i }));
    await waitFor(() => expect(screen.getByText(/TRIGGER\.md frontmatter needs name:/i)).toBeTruthy());
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("TRIGGER.md missing x-agentsfleet sub-fields blocks submit", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    await user.type(
      screen.getByLabelText(/TRIGGER\.md body/i),
      "---\nname: platform-ops\nx-agentsfleet:\n  tools:\n    - agentmail\n  budget:\n    daily_dollars: 1.0\n---\n",
    );
    await user.type(screen.getByLabelText(/SKILL\.md body/i), FIXTURE_SKILL);
    await user.click(screen.getByRole("button", { name: /create fleet/i }));
    await waitFor(() => expect(screen.getByText(/x-agentsfleet needs triggers:/i)).toBeTruthy());
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("Back and Cancel both call onBack", async () => {
    const user = userEvent.setup({ delay: null });
    renderForm();
    await user.click(screen.getByRole("button", { name: /Back to templates/i }));
    expect(onBack).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onBack).toHaveBeenCalledTimes(2);
  });
});

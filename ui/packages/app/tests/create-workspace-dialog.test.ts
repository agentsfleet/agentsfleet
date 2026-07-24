import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@agentsfleet/design-system", async (orig) => {
  const helpers = await import("./helpers/dashboard-mocks");
  return { ...helpers.designSystemCore(await orig<Record<string, unknown>>()) };
});

beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

describe("CreateWorkspaceDialog component", () => {
  type DialogProps = {
    open: boolean;
    pending: boolean;
    error: string | null;
    onOpenChange: (open: boolean) => void;
    onSubmit: (name?: string) => void | Promise<void>;
    restoreFocus?: () => void;
  };

  async function renderDialog(overrides: Partial<DialogProps> = {}) {
    const { default: CreateWorkspaceDialog } = await import(
      "../components/layout/CreateWorkspaceDialog"
    );
    const props: DialogProps = {
      open: true,
      pending: false,
      error: null,
      onOpenChange: vi.fn(),
      onSubmit: vi.fn(),
      ...overrides,
    };
    const view = render(React.createElement(CreateWorkspaceDialog, props));
    return { CreateWorkspaceDialog, props, view };
  }

  it("submits the trimmed workspace name through the native form", async () => {
    const user = userEvent.setup({ delay: null });
    const { props } = await renderDialog();
    await user.type(screen.getByLabelText("Name (optional)"), "  acme-prod  ");
    await user.click(screen.getByTestId("workspace-create-submit"));
    expect(props.onSubmit).toHaveBeenCalledWith("acme-prod");
  });

  it("omits a blank name so the server can generate one", async () => {
    const user = userEvent.setup({ delay: null });
    const { props } = await renderDialog();
    await user.click(screen.getByTestId("workspace-create-submit"));
    expect(props.onSubmit).toHaveBeenCalledWith(undefined);
  });

  it("submits with Enter without a custom key handler", async () => {
    const user = userEvent.setup({ delay: null });
    const { props } = await renderDialog();
    await user.type(screen.getByLabelText("Name (optional)"), "via-enter{Enter}");
    expect(props.onSubmit).toHaveBeenCalledWith("via-enter");
  });

  it("explains the workspace boundary and associates its label with the input", async () => {
    await renderDialog();
    expect(screen.getByText(
      "Use workspaces to organize fleets, teammates, and credentials within your tenant. Leave the name blank to generate one.",
    )).toBeTruthy();
    expect(screen.getByLabelText("Name (optional)")).toBeTruthy();
  });

  it("shows a controlled error without clearing the attempted name", async () => {
    const user = userEvent.setup({ delay: null });
    const { CreateWorkspaceDialog, props, view } = await renderDialog();
    const input = screen.getByLabelText("Name (optional)") as HTMLInputElement;
    await user.type(input, "kept-name");
    view.rerender(
      React.createElement(CreateWorkspaceDialog, {
        ...props,
        error: "Couldn't create workspace.",
      }),
    );
    expect(screen.getByTestId("workspace-create-error").textContent).toContain(
      "Couldn't create workspace.",
    );
    expect(input.value).toBe("kept-name");
  });

  it("keeps dismissal available while a request is pending", async () => {
    const user = userEvent.setup({ delay: null });
    const { props } = await renderDialog({ pending: true });
    expect((screen.getByLabelText("Name (optional)") as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByTestId("workspace-create-submit") as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByTestId("workspace-create-form").getAttribute("aria-busy")).toBe("true");
    await user.click(screen.getByRole("button", { name: "Hide" }));
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
  });

  it("cancels without submitting when idle", async () => {
    const user = userEvent.setup({ delay: null });
    const { props } = await renderDialog();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
    expect(props.onSubmit).not.toHaveBeenCalled();
  });

  it("starts with a clean uncontrolled input after close and reopen", async () => {
    const user = userEvent.setup({ delay: null });
    const { CreateWorkspaceDialog, props, view } = await renderDialog();
    await user.type(screen.getByLabelText("Name (optional)"), "draft-name");
    view.rerender(React.createElement(CreateWorkspaceDialog, { ...props, open: false }));
    view.rerender(React.createElement(CreateWorkspaceDialog, props));
    expect((screen.getByLabelText("Name (optional)") as HTMLInputElement).value).toBe("");
  });

  it("restores focus through the parent callback after closing", async () => {
    const user = userEvent.setup({ delay: null });
    const restoreFocus = vi.fn();
    const { CreateWorkspaceDialog, props, view } = await renderDialog({ restoreFocus });
    await user.click(screen.getByRole("button", { name: "Close" }));
    view.rerender(React.createElement(CreateWorkspaceDialog, { ...props, open: false }));
    await waitFor(() => expect(restoreFocus).toHaveBeenCalledOnce());
  });
});

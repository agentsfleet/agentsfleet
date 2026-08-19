import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
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
    onSubmit: (name: string) => void | Promise<void>;
    restoreFocus?: () => void;
  };

  async function renderDialog(overrides: Partial<DialogProps> = {}) {
    const { default: CreateWorkspaceDialog } =
      await import("../components/layout/CreateWorkspaceDialog");
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
    await user.type(screen.getByLabelText("Name"), "  acme-prod  ");
    await user.click(screen.getByTestId("workspace-create-submit"));
    expect(props.onSubmit).toHaveBeenCalledWith("acme-prod");
  });

  it("trims ASCII edges but preserves Unicode whitespace", async () => {
    const { props } = await renderDialog();
    const input = screen.getByLabelText("Name");
    fireEvent.change(input, {
      target: { value: " \t\u00a0acme\u3000\r\n" },
    });
    fireEvent.submit(screen.getByTestId("workspace-create-form"));

    expect(props.onSubmit).toHaveBeenCalledWith("\u00a0acme\u3000");
  });

  it("rejects a name made only of Unicode whitespace", async () => {
    const { props } = await renderDialog();
    const input = screen.getByLabelText("Name") as HTMLInputElement;
    const reportValidity = vi
      .spyOn(input, "reportValidity")
      .mockReturnValue(false);
    fireEvent.change(input, { target: { value: "\u00a0\u3000" } });
    fireEvent.submit(screen.getByTestId("workspace-create-form"));

    expect(props.onSubmit).not.toHaveBeenCalled();
    expect(input.validationMessage).toBe("Enter a workspace name.");
    expect(reportValidity).toHaveBeenCalledOnce();
  });

  it("requires a non-blank name before submitting", async () => {
    const user = userEvent.setup({ delay: null });
    const { props } = await renderDialog();
    await user.click(screen.getByTestId("workspace-create-submit"));
    expect(props.onSubmit).not.toHaveBeenCalled();
    expect((screen.getByLabelText("Name") as HTMLInputElement).required).toBe(
      true,
    );
  });

  it("rejects whitespace submitted outside native validation", async () => {
    const { props } = await renderDialog();
    const input = screen.getByLabelText("Name") as HTMLInputElement;
    const reportValidity = vi
      .spyOn(input, "reportValidity")
      .mockReturnValue(false);
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.submit(screen.getByTestId("workspace-create-form"));
    expect(props.onSubmit).not.toHaveBeenCalled();
    expect(input.validationMessage).toBe("Enter a workspace name.");
    expect(reportValidity).toHaveBeenCalledOnce();
    fireEvent.input(input, { target: { value: "a" } });
    expect(input.validationMessage).toBe("");
  });

  it("rejects a name longer than 128 Unicode code points", async () => {
    const { props } = await renderDialog();
    const input = screen.getByLabelText("Name") as HTMLInputElement;
    const reportValidity = vi
      .spyOn(input, "reportValidity")
      .mockReturnValue(false);
    fireEvent.change(input, { target: { value: "a".repeat(129) } });
    fireEvent.submit(screen.getByTestId("workspace-create-form"));
    expect(props.onSubmit).not.toHaveBeenCalled();
    expect(input.validationMessage).toBe("Use 128 characters or fewer.");
    expect(reportValidity).toHaveBeenCalledOnce();
  });

  it("rejects directional formatting and Unicode line separators", async () => {
    const { props } = await renderDialog();
    const input = screen.getByLabelText("Name") as HTMLInputElement;
    const reportValidity = vi
      .spyOn(input, "reportValidity")
      .mockReturnValue(false);
    for (const unsafe of ["safe\u202Etxt", "safe\u2028txt"]) {
      fireEvent.change(input, { target: { value: unsafe } });
      fireEvent.submit(screen.getByTestId("workspace-create-form"));
    }
    expect(props.onSubmit).not.toHaveBeenCalled();
    expect(input.validationMessage).toBe(
      "Remove control or directional formatting characters.",
    );
    expect(reportValidity).toHaveBeenCalledTimes(2);
  });

  it("submits with Enter without a custom key handler", async () => {
    const user = userEvent.setup({ delay: null });
    const { props } = await renderDialog();
    await user.type(screen.getByLabelText("Name"), "via-enter{Enter}");
    expect(props.onSubmit).toHaveBeenCalledWith("via-enter");
  });

  it("explains the workspace boundary and associates its label with the input", async () => {
    await renderDialog();
    expect(
      screen.getByText(
        "Use workspaces to organize fleets, teammates, and credentials within your organization.",
      ),
    ).toBeTruthy();
    expect(screen.getByLabelText("Name")).toBeTruthy();
  });

  it("shows a controlled error without clearing the attempted name", async () => {
    const user = userEvent.setup({ delay: null });
    const { CreateWorkspaceDialog, props, view } = await renderDialog();
    const input = screen.getByLabelText("Name") as HTMLInputElement;
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
    expect((screen.getByLabelText("Name") as HTMLInputElement).disabled).toBe(
      true,
    );
    expect(
      (screen.getByTestId("workspace-create-submit") as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(
      screen.getByTestId("workspace-create-form").getAttribute("aria-busy"),
    ).toBe("true");
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
    await user.type(screen.getByLabelText("Name"), "draft-name");
    view.rerender(
      React.createElement(CreateWorkspaceDialog, { ...props, open: false }),
    );
    view.rerender(React.createElement(CreateWorkspaceDialog, props));
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("");
  });

  it("restores focus through the parent callback after closing", async () => {
    const user = userEvent.setup({ delay: null });
    const restoreFocus = vi.fn();
    const { CreateWorkspaceDialog, props, view } = await renderDialog({
      restoreFocus,
    });
    await user.click(screen.getByRole("button", { name: "Close" }));
    view.rerender(
      React.createElement(CreateWorkspaceDialog, { ...props, open: false }),
    );
    await waitFor(() => expect(restoreFocus).toHaveBeenCalledOnce());
  });

  it("does not restore stale close focus after the dialog has reopened", async () => {
    const restoreFocus = vi.fn();
    const { CreateWorkspaceDialog, props, view } = await renderDialog({
      restoreFocus,
    });
    view.rerender(
      React.createElement(CreateWorkspaceDialog, { ...props, open: false }),
    );
    view.rerender(React.createElement(CreateWorkspaceDialog, props));
    await Promise.resolve();
    expect(restoreFocus).not.toHaveBeenCalled();
  });
});

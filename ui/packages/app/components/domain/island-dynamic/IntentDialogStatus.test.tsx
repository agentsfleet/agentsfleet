import {useState} from "react";
import {afterEach, expect, it} from "vitest";
import {cleanup, fireEvent, render, screen, within} from "@testing-library/react";
import {IntentDialogStatus} from "./IntentDialogStatus";
import {INTENT_MODULE_STATUS} from "./intent-module-loader";

afterEach(cleanup);

function Recovery() {
  const [open, setOpen] = useState(true);
  const [status, setStatus] = useState<typeof INTENT_MODULE_STATUS.error | typeof INTENT_MODULE_STATUS.loading>(INTENT_MODULE_STATUS.error);
  return <IntentDialogStatus title="Edit setup" description="Loading editor" errorMessage="Editor could not load."
    open={open} status={status} onOpenChange={setOpen} onRetry={() => setStatus(INTENT_MODULE_STATUS.loading)} />;
}

it("closes a failed editor from its footer and retries into an announced loading state", () => {
  const view = render(<Recovery />);
  fireEvent.click(within(screen.getByRole("dialog")).getByText("Close", {selector: "button"}));
  expect(screen.queryByRole("dialog")).toBeNull();
  view.unmount(); render(<Recovery />);
  expect(screen.getByRole("alert").textContent).toBe("Editor could not load.");
  fireEvent.click(screen.getByRole("button", {name: "Retry"}));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.getByRole("dialog").getAttribute("aria-busy")).toBe("true");
  expect(screen.getByText("Loading editor")).toBeTruthy();
});

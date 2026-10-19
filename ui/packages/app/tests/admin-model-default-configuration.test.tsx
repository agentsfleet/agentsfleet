import { ADMIN_MODEL } from "./helpers/catalogue-fixtures";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import MakeDefaultDialog from "@/app/(dashboard)/admin/models/components/MakeDefaultDialog";
import type { AdminModel } from "@/lib/api/admin_model_library";

const { activate } = vi.hoisted(() => ({ activate: vi.fn() }));
vi.mock("@/app/(dashboard)/admin/models/actions", () => ({ setPlatformDefaultAction: activate }));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: vi.fn() }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("platform default requires saved model configuration", () => {
  it.each(["fireworks", "openai-compatible"])("refuses an unconfigured %s row before saving a key", (provider) => {
    const model: AdminModel = { ...ADMIN_MODEL, id: "unconfigured", provider, inference: undefined };
    render(<MakeDefaultDialog model={model} onDone={vi.fn()} onOpenChange={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: "fixture-secret" } });
    expect(screen.getByText(/Edit this model to configure its inference API, URL and output limit/)).not.toBeNull();
    expect((screen.getByRole("button", { name: "Make default" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Make default" }));
    expect(activate).not.toHaveBeenCalled();
    expect(screen.getByLabelText("API key").getAttribute("type")).toBe("password");
  });
});

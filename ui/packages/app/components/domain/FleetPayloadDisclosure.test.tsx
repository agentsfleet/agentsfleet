import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FleetPayloadDisclosure } from "./FleetPayloadDisclosure";

const COMPACT_PAYLOAD = '{"event":{"id":"evt_1"},"status":"edited"}';
const FORMATTED_PAYLOAD = '{\n  "event": {\n    "id": "evt_1"\n  },\n  "status": "edited"\n}';
const RAW_PAYLOAD = "payload=not-json";
const COPY_LABEL = "Copy JSON";

function stubClipboard(writeText: (text: string) => Promise<void>) {
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
}

afterEach(() => cleanup());

describe("FleetPayloadDisclosure", () => {
  it("formats valid JSON and copies the exact displayed payload", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    const { container } = render(<FleetPayloadDisclosure json={COMPACT_PAYLOAD} />);
    const trigger = screen.getByRole("button", { name: "Details" });

    expect(container.querySelector("pre")).toBeNull();
    fireEvent.click(trigger);

    expect(container.querySelector("pre")?.textContent).toBe(FORMATTED_PAYLOAD);
    fireEvent.click(screen.getByRole("button", { name: COPY_LABEL }));
    expect(writeText).toHaveBeenCalledWith(FORMATTED_PAYLOAD);
    await screen.findByRole("button", { name: "Copied" });
  });

  it("preserves a non-JSON payload verbatim for inspection", () => {
    const { container } = render(<FleetPayloadDisclosure json={RAW_PAYLOAD} />);
    fireEvent.click(screen.getByRole("button", { name: "Details" }));

    expect(container.querySelector("pre")?.textContent).toBe(RAW_PAYLOAD);
  });

  it("falls back to the original value when formatting produces no string", () => {
    const view = render(<FleetPayloadDisclosure json={COMPACT_PAYLOAD} />);
    const stringify = vi.spyOn(JSON, "stringify").mockReturnValueOnce(undefined as never);
    view.rerender(<FleetPayloadDisclosure json={COMPACT_PAYLOAD} />);
    fireEvent.click(screen.getByRole("button", { name: "Details" }));

    expect(view.container.querySelector("pre")?.textContent).toBe(COMPACT_PAYLOAD);
    stringify.mockRestore();
  });

  it("renders the formatted payload body directly for a parent Details row", () => {
    const { container } = render(
      <FleetPayloadDisclosure json={COMPACT_PAYLOAD} inline />,
    );

    expect(screen.queryByRole("button", { name: "Details" })).toBeNull();
    expect(container.querySelector("pre")?.textContent).toBe(FORMATTED_PAYLOAD);
    expect(screen.getByRole("button", { name: COPY_LABEL })).toBeTruthy();
  });
});

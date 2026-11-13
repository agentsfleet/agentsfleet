import type { ThreadMessageLike } from "@assistant-ui/react";
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { AuiConfig, AssistantRuntimeProvider, ThreadPrimitive, Tools, defineToolkit, useExternalStoreRuntime } from "@assistant-ui/react";
import { renderFleetMessage } from "@/components/domain/fleetMessageRenderers";
import { FLEET_CHAT_CONFIG } from "@/components/domain/FleetToolRegistry";

afterEach(cleanup);

it("uses the native registered tool UI before the generic tool view", () => {
  const renderTool = vi.fn(() => <output>Native registry selected this renderer</output>);
  const config = AuiConfig({tools: Tools({toolkit: defineToolkit({release_report: {type: "backend", render: renderTool}})})});
  render(<Reply config={config} name="release_report" />);
  fireEvent.click(screen.getByRole("button", {name: /^Tool activity:/}));
  expect(screen.getByText("Native registry selected this renderer")).toBeTruthy();
  expect(renderTool).toHaveBeenCalled();
  expect(document.querySelector('[data-tool="release_report"]')).toBeNull();
});

it("registers the existing backend tool names without adding an executor to the browser", () => {
  render(<Reply config={FLEET_CHAT_CONFIG} name="file_read" />);
  fireEvent.click(screen.getByRole("button", {name: /^Tool activity:/}));
  const cell = document.querySelector('[data-tool="file_read"]');
  expect(cell).toBeTruthy();
  expect(cell?.textContent).toContain("Read");
  expect(cell?.getAttribute("data-state")).toBe("succeeded");
});

it.each(["new_backend_tool", "constructor", "__proto__"])("keeps %s readable through the native parts fallback", name => {
  render(<Reply config={FLEET_CHAT_CONFIG} name={name} />);
  fireEvent.click(screen.getByRole("button", {name: /^Tool activity:/}));
  const cell = document.querySelector('[data-tool]');
  expect(cell?.getAttribute("data-tool")).toBe(name);
  expect(cell?.textContent).toContain("Used tool");
  fireEvent.click(screen.getByRole("button", {name: /^Used tool/}));
  expect(screen.getByText(`Action · ${name}`)).toBeTruthy();
  expect(screen.queryByText(/not supported|unsupported/i)).toBeNull();
});

function Reply({config, name}: {config: typeof FLEET_CHAT_CONFIG; name: string}) {
  const messages: ThreadMessageLike[] = [{id: "native-reply", role: "assistant", createdAt: new Date(0), status: {type: "complete", reason: "stop"},
    content: [{type: "tool-call", toolCallId: "read", toolName: name, args: {path: "release.yaml"}, argsText: '{"path":"release.yaml"}',
      result: {status: "succeeded", outputHead: "ready: true", outputLineCount: 1}}]}];
  const runtime = useExternalStoreRuntime<ThreadMessageLike>({messages, isRunning: false, convertMessage: message => message, onNew: async () => {}});
  return <AssistantRuntimeProvider runtime={runtime} config={config}><ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages></AssistantRuntimeProvider>;
}

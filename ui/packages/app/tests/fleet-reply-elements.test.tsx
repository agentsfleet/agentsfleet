import type { ThreadMessageLike } from "@assistant-ui/react";
import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import { renderFleetMessage } from "@/components/domain/fleetMessageRenderers";

afterEach(cleanup);

it("renders code and links inside an assistant reply through its registered text renderer", () => {
  const {container} = render(<Reply content={[{type: "text", text: [
    "Run `make lint-app` and read [the result](https://example.test/results).", "",
    "```ts", "const status = 'ready';", "```",
  ].join("\n")}]} />);
  expect(screen.getByText("make lint-app").tagName).toBe("CODE");
  expect(container.querySelector("pre code")?.textContent).toContain("const status = 'ready';");
  const link = screen.getByRole("link", {name: "the result"});
  expect(link.getAttribute("href")).toBe("https://example.test/results");
  expect(link.getAttribute("rel")).toContain("noopener");
});

it("uses a readable source link when a reply part has no specialized view", () => {
  render(<Reply content={[{type: "source", sourceType: "url", id: "source", title: "Release report", url: "https://example.test/report"}]} />);
  expect(screen.getByRole("link", {name: "Release report"}).getAttribute("href")).toBe("https://example.test/report");
  expect(screen.queryByText(/not supported|unavailable/i)).toBeNull();
});

it.each(["javascript:alert(1)", "data:text/html,unsafe", "/private", "file:///private"])("does not make %s a source action", url => {
  const {container} = render(<Reply content={[{type: "source", sourceType: "url", id: "source", title: "Release report", url}]} />);
  expect(screen.getByText("Release report · Preview unavailable.")).toBeTruthy();
  expect(container.querySelector("a, iframe, script")).toBeNull();
  expect(container.textContent).not.toContain(url);
});

it("shows the document name without disclosing its encoded contents", () => {
  const {container} = render(<Reply content={[{type: "file", filename: "release 東京.txt", mimeType: "text/plain", data: "private-file-body"}]} />);
  expect(screen.getByText("release 東京.txt · Preview unavailable.")).toBeTruthy();
  expect(container.textContent).not.toContain("private-file-body");
  expect(container.querySelector("a, iframe")).toBeNull();
});

it("keeps an opaque data part visible without rendering arbitrary fields or component specs", () => {
  const {container} = render(<Reply content={[{type: "data", name: "future-result", data: {
    component: "RunCommand", command: "do-not-execute", private: "do-not-disclose", html: "<script>unsafe</script>",
  }}]} />);
  expect(screen.getByText("Activity details · Preview unavailable.")).toBeTruthy();
  expect(container.textContent).not.toMatch(/do-not-execute|do-not-disclose|unsafe/);
  expect(container.querySelector("script, iframe, button[data-execute]")).toBeNull();
});

it("keeps an image label readable without fetching its body automatically", () => {
  const {container} = render(<Reply content={[{type: "image", image: "https://example.test/screenshot.png", filename: "Run screenshot"}]} />);
  expect(screen.getByText("Run screenshot · Preview unavailable.")).toBeTruthy();
  expect(container.querySelector("img, script, a")).toBeNull();
  expect(container.textContent).not.toContain("https://example.test/screenshot.png");
});

function Reply({content}: {content: ThreadMessageLike["content"]}) {
  const messages: ThreadMessageLike[] = [{id: "reply", role: "assistant", content, createdAt: new Date(0),
    status: {type: "complete", reason: "stop"}, metadata: {custom: {actor: "fleet", status: "processed"}}}];
  const runtime = useExternalStoreRuntime<ThreadMessageLike>({messages, isRunning: false, convertMessage: message => message, onNew: async () => {}});
  return <AssistantRuntimeProvider runtime={runtime}><ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages></AssistantRuntimeProvider>;
}

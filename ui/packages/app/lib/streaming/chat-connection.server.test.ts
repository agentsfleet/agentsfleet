// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { ChatConnection, CHAT_CONNECTION_STATUS } from "./chat-connection";

beforeEach(() => FakeEventSource.install());
afterEach(() => {vi.restoreAllMocks(); FakeEventSource.uninstall();});

it("opens without page recovery listeners where there is no document, and closes cleanly", () => {
  const callbacks = {onFrame: vi.fn(() => true), onOpen: vi.fn(), onInterrupted: vi.fn(), onStatus: vi.fn(), onHeartbeat: vi.fn()};
  const connection = new ChatConnection({url: "/private-stream", events: ["message.delta"], ...callbacks});
  expect(typeof document).toBe("undefined");
  connection.start();
  const source = FakeEventSource.instances[0];
  source?.open();
  source?.emitNamed("message.delta", "answer");
  expect(callbacks.onFrame).toHaveBeenCalledWith("message.delta", "answer");
  expect(callbacks.onStatus).toHaveBeenLastCalledWith(CHAT_CONNECTION_STATUS.LIVE);
  connection.close();
  expect(FakeEventSource.instances).toHaveLength(1);
});

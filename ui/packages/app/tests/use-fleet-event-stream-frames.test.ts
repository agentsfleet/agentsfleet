import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, waitFor } from "@testing-library/react";
import { CONNECTION_STATUS } from "../components/domain/useFleetEventStream";
import { __resetRegistryForTests } from "@/tests/helpers/fleet-streams";
import { OUTCOME } from "@/lib/events/event-summary";
import type { LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { FakeEventSource } from "./helpers/fake-event-source";
import { mount } from "./helpers/fleet-stream-hook-fixtures";

const UNKNOWN_FRAME_KIND = "future_kind_we_dont_know";

describe("useFleetEventStream — conversion and frame edges", () => {
  beforeEach(() => {
    FakeEventSource.install();
    __resetRegistryForTests();
  });

  afterEach(() => {
    cleanup();
    __resetRegistryForTests();
    FakeEventSource.uninstall();
  });

  it("convertEvent carries the trigger in content and the outcome, never the reply, in metadata", () => {
    const { result } = mount();
    const msg = result.current.convertEvent({
      id: "evt_silent",
      role: "system",
      actor: "github-app",
      text: "opened · owner/repo#7",
      reply: "",
      outcome: OUTCOME.COMPLETED,
      failureLabel: null,
      failureDetail: null,
      createdAt: new Date(0),
      status: "processed",
    });
    // Content is the trigger and the outcome is metadata, so a reply-less turn
    // still says what happened. The reply is the reply message's content, and
    // carrying it here would change the trigger on every streamed word.
    expect(msg.content).toEqual([{ type: "text", text: "opened · owner/repo#7" }]);
    expect(msg.metadata?.custom).not.toHaveProperty("reply");
    expect(msg.metadata?.custom?.["outcome"]).toBe(OUTCOME.COMPLETED);
  });

  it("convertEvent produces an assistant-ui ThreadMessageLike with custom metadata", () => {
    const { result } = mount();
    const msg = result.current.convertEvent({
      id: "evt_x",
      role: "system",
      actor: "webhook:github",
      text: "workflow_run failure",
      reply: "",
      failureLabel: null,
      failureDetail: null,
      outcome: OUTCOME.COMPLETED,
      createdAt: new Date(0),
      status: "processed",
      custom: { requestJson: '{"action":"workflow_run"}' },
    });
    expect(msg.role).toBe("system");
    expect(msg.id).toBe("evt_x");
    expect(msg.content).toEqual([{ type: "text", text: "workflow_run failure" }]);
    expect(msg.metadata?.custom?.actor).toBe("webhook:github");
    expect(msg.metadata?.custom?.requestJson).toBe('{"action":"workflow_run"}');
  });

  it("ignores SSE frames with malformed JSON", () => {
    const { result } = mount();
    act(() => {
      FakeEventSource.instances[0]!.emitRaw("this is not json");
    });
    expect(result.current.events).toEqual([]);
  });

  // ── Robustness: invalid/error paths ─────────────────────────────────────

  it("creates a fresh event row when CHUNK arrives before EVENT_RECEIVED", async () => {
    const { result } = mount();
    act(() => {
      FakeEventSource.instances[0]!.open();
      FakeEventSource.instances[0]!.heartbeat();
    });
    await waitFor(() =>
      expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.LIVE),
    );
    act(() => {
      FakeEventSource.instances[0]!.emit({
        kind: FRAME_KIND.CHUNK,
        event_id: "evt_orphan",
        text: "partial body without a header frame",
        text_kind: "answer",
        stream_seq: 0,
        stream_start: true,
        stream_contiguous: true,
      } as LiveFrame);
    });
    await waitFor(() => {
      expect(result.current.events.length).toBe(1);
      expect(result.current.events[0]!.id).toBe("evt_orphan");
      expect(result.current.events[0]!.reply).toBe(
        "partial body without a header frame",
      );
    });
  });

  it("drops SSE frames that parse to non-object values", async () => {
    const { result } = mount();
    const inputs: unknown[] = [null, 42, '"a string"', "[1,2,3]"];
    act(() => {
      for (const raw of inputs) {
        FakeEventSource.instances[0]!.emitRaw(
          typeof raw === "string" ? raw : JSON.stringify(raw),
        );
      }
    });
    expect(result.current.events).toEqual([]);
  });

  it("drops SSE frames with unknown kind (default-switch arm)", async () => {
    const { result } = mount();
    act(() => {
      FakeEventSource.instances[0]!.emitRaw(
        JSON.stringify({ kind: UNKNOWN_FRAME_KIND }),
      );
    });
    expect(result.current.events).toEqual([]);
  });

});

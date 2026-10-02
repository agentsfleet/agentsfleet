import React, { Profiler } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";

import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { OUTCOME } from "@/lib/events/event-summary";
import { AGENTSFLEET_EVENT_STATUS } from "@/lib/streaming/fleet-stream-row";
import { __resetPendingSendsForTests } from "@/lib/streaming/pending-sends";

// The thread shell under a streamed reply, through the real runtime: every
// flush hands the thread a new event array, and the counters below say who
// rendered because of it.
const shell = vi.hoisted(() => ({
  user: "user_flush",
  viewportRenders: 0,
  composerCommits: 0,
  adapters: [] as unknown[],
  stream: vi.fn(),
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: vi.fn() }),
  usePathname: () => "/w/ws_flush/fleets/fleet_flush",
}));
vi.mock("@/lib/auth/client", () => ({
  useCurrentUser: () => ({ isLoaded: true, isSignedIn: true, userId: shell.user, emailAddress: null, hasImage: false }),
}));
vi.mock("@/lib/api/fleet-steer", () => ({ postSteer: vi.fn() }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({}));

// The runtime is handed its adapter once per thread render.
vi.mock("@assistant-ui/react", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@assistant-ui/react")>();
  return {
    ...actual,
    useExternalStoreRuntime: (adapter: Parameters<typeof actual.useExternalStoreRuntime>[0]) => {
      shell.adapters.push(adapter);
      return actual.useExternalStoreRuntime(adapter);
    },
  };
});

// The viewport is memoised; a memo with the same props gate in front of it
// counts exactly the renders that get through to it, one commit each.
vi.mock("@/components/domain/FleetThreadViewport", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/domain/FleetThreadViewport")>();
  const Counted = React.memo(function Counted(props: React.ComponentProps<typeof actual.FleetThreadViewport>) {
    React.useLayoutEffect(() => {
      shell.viewportRenders += 1;
    });
    return <actual.FleetThreadViewport {...props} />;
  });
  return { ...actual, FleetThreadViewport: Counted };
});

// A commit anywhere in the composer's subtree — its own subscriptions or its
// parent's render — reaches this profiler.
vi.mock("@/components/domain/SteerComposer", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/domain/SteerComposer")>();
  return {
    ...actual,
    SteerComposer: (props: React.ComponentProps<typeof actual.SteerComposer>) => (
      <Profiler id="composer" onRender={() => { shell.composerCommits += 1; }}>
        <actual.SteerComposer {...props} />
      </Profiler>
    ),
  };
});

vi.mock("@/components/domain/useFleetEventStream", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/domain/useFleetEventStream")>();
  return { ...actual, useFleetEventStream: shell.stream };
});

import { FleetThread } from "./FleetThread";
import { CONNECTION_STATUS, convertEvent } from "./useFleetEventStream";
import { ANNOUNCEMENT_HOLD_MS, SETTLED_REPLY_STATUS, SPOKEN_REPLY_MAX_CHARS, spokenSummary } from "./FleetReplyBody";

const FLUSHES = 100;
const EVENT_ID = "evt_flush";
const STEER_ACTOR = `steer:${shell.user}`;
const FINAL_WORD = "done.";
const SENDER = "reviewer";
const AGAIN_ID = "evt_flush_again";

// Stable across flushes, as the registry's callbacks are.
const STABLE = {
  connectionStatus: CONNECTION_STATUS.LIVE,
  isRunning: false,
  appendOptimistic: vi.fn(),
  reconcileOptimistic: vi.fn(),
  discardOptimistic: vi.fn(),
  retryConnection: vi.fn(),
  convertEvent,
};

function turn(reply: string, status: FleetEvent["status"]): FleetEvent {
  return {
    id: EVENT_ID,
    role: "user",
    actor: STEER_ACTOR,
    text: "Review the change",
    reply,
    outcome: OUTCOME.COMPLETED,
    failureLabel: null,
    failureDetail: null,
    createdAt: new Date(0),
    status,
  };
}

function thread() {
  return <FleetThread workspaceId="ws_flush" fleetId="fleet_flush" senderLabel={SENDER} initial={[]} viewer={shell.user} senderNames={[]} />;
}

function flush(view: ReturnType<typeof render>, event: FleetEvent) {
  shell.stream.mockReturnValue({ ...STABLE, events: [event] });
  view.rerender(thread());
}

function streamedWords(count: number): string {
  return Array.from({ length: count }, (_, word) => `word${word}`).join(" ");
}

beforeEach(() => {
  __resetPendingSendsForTests();
  shell.viewportRenders = 0;
  shell.composerCommits = 0;
  shell.adapters.length = 0;
});

afterEach(cleanup);

describe("a streamed reply and the thread around it", () => {
  it("test_flush_leaves_the_shell_alone: 100 text flushes render neither the viewport nor the composer", () => {
    shell.stream.mockReturnValue({ ...STABLE, events: [turn("", AGENTSFLEET_EVENT_STATUS.RECEIVED)] });
    const view = render(thread());
    expect(shell.viewportRenders).toBeGreaterThan(0);
    expect(shell.composerCommits).toBeGreaterThan(0);
    shell.viewportRenders = 0;
    shell.composerCommits = 0;

    for (let count = 1; count <= FLUSHES; count += 1) {
      flush(view, turn(streamedWords(count), AGENTSFLEET_EVENT_STATUS.RECEIVED));
    }

    expect(screen.getByText(new RegExp(`word${FLUSHES - 1}$`))).toBeTruthy();
    expect(shell.viewportRenders).toBe(0);
    expect(shell.composerCommits).toBe(0);
  });

  it("test_backfill_walk_notifies_once: the transcript announces a settled reply once, and never while it streams", () => {
    shell.stream.mockReturnValue({ ...STABLE, events: [turn("", AGENTSFLEET_EVENT_STATUS.RECEIVED)] });
    const view = render(thread());
    const status = screen.getByTestId(SETTLED_REPLY_STATUS);
    // Every distinct text the polite status held, read after each flush.
    const announced: string[] = [];
    const listen = () => {
      const text = status.textContent ?? "";
      if (text !== announced.at(-1)) announced.push(text);
    };
    listen();

    // The log is not a live region, so the growing answer is never read out.
    expect(screen.getByRole("log").getAttribute("aria-live")).toBe("off");
    for (let count = 1; count <= FLUSHES; count += 1) {
      flush(view, turn(streamedWords(count), AGENTSFLEET_EVENT_STATUS.RECEIVED));
      listen();
    }
    const answer = `${streamedWords(FLUSHES)} ${FINAL_WORD}`;
    flush(view, turn(answer, AGENTSFLEET_EVENT_STATUS.PROCESSED));
    listen();
    // A settled reply whose text is corrected afterwards is not read out again.
    flush(view, turn(`${answer} ${FINAL_WORD}`, AGENTSFLEET_EVENT_STATUS.PROCESSED));
    listen();

    // Read as a bounded summary: the row holds the whole answer.
    expect(announced).toEqual(["", `${SENDER}: ${spokenSummary(answer)}`]);
    expect(spokenSummary(answer).length).toBeLessThanOrEqual(SPOKEN_REPLY_MAX_CHARS + 1);
  });

  it("announces a second reply that settles with the same words as the first", () => {
    const again = (status: FleetEvent["status"]): FleetEvent => ({ ...turn(FINAL_WORD, status), id: AGAIN_ID });
    const first = turn(FINAL_WORD, AGENTSFLEET_EVENT_STATUS.PROCESSED);
    shell.stream.mockReturnValue({ ...STABLE, events: [turn(FINAL_WORD, AGENTSFLEET_EVENT_STATUS.RECEIVED)] });
    const view = render(thread());
    flush(view, first);
    const status = screen.getByTestId(SETTLED_REPLY_STATUS);
    const firstWords = status.firstElementChild;
    expect(firstWords?.textContent).toBe(`${SENDER}: ${FINAL_WORD}`);

    shell.stream.mockReturnValue({ ...STABLE, events: [first, again(AGENTSFLEET_EVENT_STATUS.RECEIVED)] });
    view.rerender(thread());
    shell.stream.mockReturnValue({ ...STABLE, events: [first, again(AGENTSFLEET_EVENT_STATUS.PROCESSED)] });
    view.rerender(thread());
    // The same words in a new text node: a screen reader hears them again.
    expect(status.firstElementChild?.textContent).toBe(`${SENDER}: ${FINAL_WORD}`);
    expect(status.firstElementChild).not.toBe(firstWords);
  });

  it("clears the status once the announcement has had time to be read", () => {
    vi.useFakeTimers();
    shell.stream.mockReturnValue({ ...STABLE, events: [turn(FINAL_WORD, AGENTSFLEET_EVENT_STATUS.RECEIVED)] });
    const view = render(thread());
    flush(view, turn(FINAL_WORD, AGENTSFLEET_EVENT_STATUS.PROCESSED));
    const status = screen.getByTestId(SETTLED_REPLY_STATUS);
    expect(status.textContent).toBe(`${SENDER}: ${FINAL_WORD}`);
    act(() => {
      vi.advanceTimersByTime(ANNOUNCEMENT_HOLD_MS);
    });
    expect(status.textContent).toBe("");
  });

  it("keeps the runtime's adapter across a render that changed no message", () => {
    const events = [turn(FINAL_WORD, AGENTSFLEET_EVENT_STATUS.PROCESSED)];
    shell.stream.mockReturnValue({ ...STABLE, events });
    const view = render(thread());
    shell.stream.mockReturnValue({ ...STABLE, connectionStatus: CONNECTION_STATUS.RECONNECTING, events });
    view.rerender(thread());
    expect(shell.adapters.length).toBeGreaterThan(1);
    expect(new Set(shell.adapters).size).toBe(1);
  });

  it("does not announce a reply that was already settled when the transcript opened", () => {
    shell.stream.mockReturnValue({ ...STABLE, events: [turn(FINAL_WORD, AGENTSFLEET_EVENT_STATUS.PROCESSED)] });
    render(thread());
    expect(screen.getByTestId(SETTLED_REPLY_STATUS).textContent).toBe("");
  });
});

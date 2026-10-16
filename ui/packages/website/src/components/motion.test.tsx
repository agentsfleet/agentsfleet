import { createRef } from "react";
import { act, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import HowItWorks from "./HowItWorks";
import { useMotionEnvironment } from "./use-motion-environment";
import { DEFAULT_JOB } from "../lib/example-jobs";
import { createChatStory, storyFrame } from "../lib/chat-story";

let intersect: IntersectionObserverCallback = () => {};
let observer: IntersectionObserver | null = null;
let mediaChanged: () => void = () => {};
let staticMode = false;
const disconnect = vi.fn();
const removeMedia = vi.fn();
const CONVERSATION_LABEL = "Example conversation";
const SCROLL_HEIGHT = 900;
const VIEWPORT_HEIGHT = 440;
const HISTORY_TOP = 160;
const JUMP_LABEL = "Jump to latest";
const INITIAL_TYPING_MS = 1000;
const READ_FULL_LABEL = "Read full conversation";
const PLAY_LABEL = "Play";
beforeEach(() => {
  staticMode = false;
  disconnect.mockClear(); removeMedia.mockClear();
  vi.useFakeTimers();
  Object.defineProperty(window, "matchMedia", { configurable: true, value: vi.fn(() => ({
    get matches() { return staticMode; },
    addEventListener: (_type: string, callback: () => void) => { mediaChanged = callback; },
    removeEventListener: removeMedia,
  })) });
  vi.stubGlobal("IntersectionObserver", class implements IntersectionObserver {
    root = null; rootMargin = "0px"; scrollMargin = "0px"; thresholds = [0.25];
    constructor(callback: IntersectionObserverCallback) { intersect = callback; observer = this; }
    observe() {} unobserve() {} takeRecords() { return []; } disconnect = disconnect;
  });
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
function visible(value: boolean) {
  if (!observer) throw new Error("No observer attached");
  const entry = { isIntersecting: value, intersectionRatio: Number(value), target: document.body,
    boundingClientRect: new DOMRect(), rootBounds: null, intersectionRect: new DOMRect(), time: 0 };
  const currentObserver = observer;
  act(() => intersect([entry], currentObserver));
}
function tick(ms = 4000) { act(() => vi.advanceTimersByTime(ms)); }

it("types and appends a looping journey while retaining earlier messages", () => {
  render(<HowItWorks job={DEFAULT_JOB} />);
  visible(true); tick(INITIAL_TYPING_MS);
  const conversation = screen.getByRole("figure");
  expect(conversation).toHaveTextContent("Watch agentsfleet/linkwarden");
  expect(conversation).not.toHaveTextContent(DEFAULT_JOB.finding);
  tick(8000);
  expect(conversation).toHaveTextContent(DEFAULT_JOB.request);
  const duration = createChatStory(DEFAULT_JOB).duration;
  tick(duration - 9000 - 50);
  expect(conversation).toHaveTextContent(DEFAULT_JOB.report);
  tick(100);
  expect(conversation).toHaveAttribute("data-cycle", "1");
  expect(conversation).not.toHaveTextContent(DEFAULT_JOB.report);
});
it("freezes text offscreen, in hidden tabs, and on manual pause", () => {
  render(<HowItWorks job={DEFAULT_JOB} />);
  visible(true); tick(INITIAL_TYPING_MS);
  const conversation = screen.getByRole("figure");
  const text = conversation.textContent;
  visible(false); tick(); expect(conversation.textContent).toBe(text);
  visible(true);
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  fireEvent(document, new Event("visibilitychange")); tick(); expect(conversation.textContent).toBe(text);
  hidden.mockReturnValue(false); fireEvent(document, new Event("visibilitychange"));
  fireEvent.click(screen.getByRole("button", { name: "Pause" })); tick(); expect(conversation.textContent).toBe(text);
  fireEvent.click(screen.getByRole("button", { name: PLAY_LABEL })); tick(); expect(screen.getByRole("figure").textContent).not.toBe(text);
});
it("shows meaningful loader text before a fleet response", () => {
  const story = createChatStory(DEFAULT_JOB);
  const reply = story.steps[1];
  if (!reply) throw new Error("Missing setup reply");
  render(<HowItWorks job={DEFAULT_JOB} />);
  visible(true); tick(reply.startsAt + 50);
  expect(screen.getByText("Understanding the job…")).toBeInTheDocument();
  tick(2000);
  expect(screen.queryByText("Understanding the job…")).toBeNull();
  expect(screen.getByRole("figure")).toHaveTextContent(DEFAULT_JOB.question);
});
it("keeps the journey running when ordinary wheel or touch reaches the chat", () => {
  render(<HowItWorks job={DEFAULT_JOB} />);
  const story = createChatStory(DEFAULT_JOB);
  const reply = story.steps[1];
  if (!reply) throw new Error("Missing setup reply");
  visible(true); tick(reply.startsAt + 50);
  const conversation = screen.getByRole("log", { name: CONVERSATION_LABEL });
  fireEvent.wheel(conversation, { deltaY: 12 });
  fireEvent.touchStart(conversation);
  expect(screen.getByTestId("how-it-works")).toHaveAttribute("data-running", "true");
  tick(story.duration);
  expect(screen.getByRole("figure")).toHaveAttribute("data-cycle", "1");
});
it("keeps the reader's scroll when visibility resumes playback automatically", () => {
  render(<HowItWorks job={DEFAULT_JOB} />);
  visible(true); tick();
  const conversation = screen.getByRole("log", { name: CONVERSATION_LABEL });
  Object.defineProperties(conversation, {
    scrollHeight: { value: SCROLL_HEIGHT },
    clientHeight: { value: VIEWPORT_HEIGHT },
  });
  conversation.scrollTop = HISTORY_TOP;
  fireEvent.scroll(conversation); tick();
  expect(conversation.scrollTop).toBe(HISTORY_TOP);
  visible(false); tick(); visible(true); tick();
  expect(conversation.scrollTop).toBe(HISTORY_TOP);
  expect(screen.getByTestId("how-it-works")).toHaveAttribute("data-running", "true");
  conversation.scrollTo = vi.fn();
  fireEvent.click(screen.getByRole("button", { name: JUMP_LABEL }));
  expect(conversation.scrollTo).toHaveBeenCalledWith({ top: SCROLL_HEIGHT, behavior: "smooth" });
  expect(screen.queryByRole("button", { name: JUMP_LABEL })).toBeNull();
  expect(screen.getByRole("button", { name: "Pause" })).toHaveFocus();
  expect(screen.getByTestId("how-it-works")).toHaveAttribute("data-running", "true");
});
it("makes all output readable immediately and keeps inspection focused until play", () => {
  render(<HowItWorks job={DEFAULT_JOB} />);
  visible(true);
  const reveal = screen.getByRole("button", { name: READ_FULL_LABEL });
  act(() => reveal.focus()); fireEvent.click(reveal);
  expect(screen.queryByRole("button", { name: READ_FULL_LABEL })).toBeNull();
  expect(screen.getByRole("button", { name: PLAY_LABEL })).toHaveFocus();
  const source = screen.getByRole("button", { name: "E2 · GitHub" });
  act(() => source.focus()); fireEvent.click(source); tick(90000);
  expect(source).toHaveFocus();
  expect(screen.getByRole("region", { name: "Sample evidence" })).toHaveTextContent("Number(process.env.BROWSER_TIMEOUT) || 5");
  expect(screen.getByTestId("how-it-works")).toHaveAttribute("data-running", "false");
  expect(screen.getByText(DEFAULT_JOB.report)).toBeInTheDocument();
});
it("lets keyboard readers focus evidence without playback moving their place", () => {
  render(<HowItWorks job={DEFAULT_JOB} />);
  visible(true); tick();
  fireEvent.click(screen.getByRole("button", { name: READ_FULL_LABEL }));
  const conversation = screen.getByRole("log", { name: CONVERSATION_LABEL });
  const evidence = screen.getByRole("button", { name: "E2 · GitHub" });
  conversation.scrollTop = HISTORY_TOP;
  act(() => evidence.focus());
  const text = conversation.textContent;
  tick();
  expect(evidence).toHaveFocus();
  expect(conversation.textContent).toBe(text);
  expect(conversation.scrollTop).toBe(HISTORY_TOP);
  expect(screen.getByRole("button", { name: PLAY_LABEL })).toBeInTheDocument();
});
it("closes evidence when playback restarts and keeps focus on the play control", () => {
  render(<HowItWorks job={DEFAULT_JOB} />);
  visible(true);
  fireEvent.click(screen.getByRole("button", { name: READ_FULL_LABEL }));
  const citation = screen.getByRole("button", { name: "E2 · GitHub" });
  act(() => citation.focus()); fireEvent.click(citation);
  const play = screen.getByRole("button", { name: PLAY_LABEL });
  act(() => play.focus()); fireEvent.click(play);
  expect(screen.queryByRole("region", { name: "Sample evidence" })).toBeNull();
  expect(play).toHaveFocus();
  expect(screen.getByTestId("how-it-works")).toHaveAttribute("data-running", "true");
});
it("honors a changed reduced-motion preference and releases all observers and timers", () => {
  const view = render(<HowItWorks job={DEFAULT_JOB} />);
  visible(true);
  act(() => { staticMode = true; mediaChanged(); });
  expect(screen.getByText(DEFAULT_JOB.report)).toBeInTheDocument();
  expect(screen.getByTestId("how-it-works")).toHaveAttribute("data-running", "false");
  view.unmount();
  expect(disconnect).toHaveBeenCalled();
  expect(removeMedia).toHaveBeenCalledWith("change", mediaChanged);
  expect(vi.getTimerCount()).toBe(0);
});
it("does not animate a missing element or empty intersection entry", () => {
  const { result } = renderHook(() => useMotionEnvironment(createRef<Element>()));
  if (!observer) throw new Error("No observer attached");
  const currentObserver = observer;
  act(() => intersect([], currentObserver));
  expect(result.current.active).toBe(false);
});
it("does not erase previous replies when the next answer is still pending", () => {
  const story = createChatStory(DEFAULT_JOB);
  const reply = story.steps[3];
  if (!reply) throw new Error("Missing refined setup");
  const frame = storyFrame(story, reply.startsAt + 50);
  expect(frame.pending?.activity).toBe("Preparing the setup");
  expect(frame.entries.map((entry) => entry.text)).toContain(DEFAULT_JOB.request);
  expect(frame.entries).toHaveLength(3);
});

import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

type Store = {getState: () => {turnAnchor: "top"; element: {viewport: HTMLElement; anchor: HTMLElement; target: HTMLElement};
  targetConfig: {tallerThan: number; visibleHeight: number}; topAnchorTurn: {anchorId: string; targetId: string}};
  subscribe: (listener: () => void) => () => void};
const require = createRequire(import.meta.url);
const moduleUrl = pathToFileURL(join(dirname(require.resolve("@assistant-ui/react")), "primitives/thread/topAnchor/mountTopAnchorReserve.js")).href;
const {mountTopAnchorReserve} = await import(/* @vite-ignore */ moduleUrl) as {mountTopAnchorReserve: (store: Store) => () => void};
let resized: () => void;
let mutated: () => void;
let cleanup: (() => void) | undefined;
const frames = new Map<number, FrameRequestCallback>();
class Resize {
  constructor(callback: () => void) {resized = callback;}
  observe(): void { /* Controlled geometry below. */ }
  disconnect(): void { /* No live observer in this unit test. */ }
}
class Mutation {
  constructor(callback: () => void) {mutated = callback;}
  observe(): void { /* Mutations are delivered explicitly. */ }
  disconnect(): void { /* No live observer in this unit test. */ }
}
beforeEach(() => {
  frames.clear();
  vi.stubGlobal("ResizeObserver", Resize); vi.stubGlobal("MutationObserver", Mutation);
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {frames.set(1, callback); return 1;});
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
});
afterEach(() => {cleanup?.(); cleanup = undefined; document.body.replaceChildren(); vi.unstubAllGlobals();});
function frame(): void {const callbacks = [...frames.values()]; frames.clear(); for (const callback of callbacks) callback(0);}
function mounted() {
  const viewport = document.createElement("div");
  const anchor = document.createElement("div");
  const target = document.createElement("div");
  document.body.append(viewport); viewport.append(anchor, target);
  for (const [key, value] of Object.entries({offsetTop: 0, clientHeight: 400, scrollHeight: 560})) Object.defineProperty(viewport, key, {value, configurable: true});
  Object.defineProperty(anchor, "offsetTop", {value: 220}); Object.defineProperty(anchor, "offsetHeight", {value: 64});
  viewport.scrollTo = vi.fn();
  const getState = vi.fn(() => ({turnAnchor: "top" as const, element: {viewport, anchor, target},
    targetConfig: {tallerThan: 160, visibleHeight: 96}, topAnchorTurn: {anchorId: "user", targetId: "assistant"}}));
  cleanup = mountTopAnchorReserve({getState, subscribe: () => () => undefined});
  frame();
  return {getState, viewport};
}

// Value: a burst of streamed mutations must read layout once per frame, while resize corrections remain immediate.
it("coalesces streamed mutations into one layout pass", () => {
  const {getState} = mounted();
  expect(getState).toHaveBeenCalledTimes(1);
  for (let index = 0; index < 20; index++) mutated();
  expect(getState).toHaveBeenCalledTimes(1);
  expect(frames.size).toBe(1);
  frame();
  expect(getState).toHaveBeenCalledTimes(2);
});

it("applies a viewport resize before waiting for another frame", () => {
  const {getState, viewport} = mounted();
  Object.defineProperty(viewport, "clientHeight", {value: 300});
  resized();
  expect(getState).toHaveBeenCalledTimes(2);
  expect(frames.size).toBe(0);
});

it("cancels queued streaming work when the thread unmounts", () => {
  const {getState} = mounted();
  mutated(); cleanup?.(); cleanup = undefined;
  frame();
  expect(getState).toHaveBeenCalledTimes(1);
});

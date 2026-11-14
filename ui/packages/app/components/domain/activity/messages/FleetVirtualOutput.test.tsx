import type { HTMLAttributes, RefObject } from "react";
import type { ListImperativeAPI } from "react-window";
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FleetVirtualOutput } from "./FleetVirtualOutput";

const windowed = vi.hoisted(() => ({scroll: vi.fn(), rendered: () => {}, ref: {current: null} as RefObject<ListImperativeAPI | null>}));
type FakeListProps = Pick<HTMLAttributes<HTMLElement>, "onKeyDown" | "onWheel" | "onPointerDown" | "onTouchStart"> & {
  listRef: RefObject<ListImperativeAPI | null>; onRowsRendered: () => void;
};
vi.mock("react-window", async () => {
  const {useLayoutEffect, useRef} = await import("react");
  return {useListRef: () => useRef(null), useDynamicRowHeight: () => 24,
    List: ({listRef, onRowsRendered, onKeyDown, onWheel, onPointerDown, onTouchStart}: FakeListProps) => {
      useLayoutEffect(() => {
        listRef.current = {element: null, scrollToRow: windowed.scroll}; windowed.ref = listRef;
        return () => {listRef.current = null;};
      }, [listRef]);
      windowed.rendered = onRowsRendered;
      return <button type="button" aria-label="Windowed output test boundary" onKeyDown={onKeyDown} onWheel={onWheel} onPointerDown={onPointerDown} onTouchStart={onTouchStart} />;
    }};
});
afterEach(() => {cleanup(); vi.clearAllMocks();});
const rows = [{number: 1, text: "first"}, {number: 2, text: "middle"}, {number: 3, text: "last"}];

it("keeps Home and End attached to the requested row while wrapped heights settle", () => {
  render(<FleetVirtualOutput rows={rows} />);
  const list = screen.getByRole("button", {name: "Windowed output test boundary"});
  act(windowed.rendered); expect(windowed.scroll).not.toHaveBeenCalled();
  expect(fireEvent.keyDown(list, {key: "End"})).toBe(false);
  expect(windowed.scroll).toHaveBeenLastCalledWith({index: 2, align: "end", behavior: "instant"});
  act(windowed.rendered);
  expect(windowed.scroll).toHaveBeenCalledTimes(2);
  expect(fireEvent.keyDown(list, {key: "Home"})).toBe(false);
  expect(windowed.scroll).toHaveBeenLastCalledWith({index: 0, align: "start", behavior: "instant"});
  windowed.ref.current = null;
  act(windowed.rendered); expect(windowed.scroll).toHaveBeenCalledTimes(3);
});

it.each(["wheel", "pointerDown", "touchStart", "keyDown"] as const)("releases keyboard anchoring on subsequent %s input", input => {
  render(<FleetVirtualOutput rows={rows} />);
  const list = screen.getByRole("button", {name: "Windowed output test boundary"});
  fireEvent.keyDown(list, {key: "End"});
  if (input === "keyDown") expect(fireEvent.keyDown(list, {key: "ArrowUp"})).toBe(true);
  else fireEvent[input](list);
  act(windowed.rendered);
  expect(windowed.scroll).toHaveBeenCalledTimes(1);
});

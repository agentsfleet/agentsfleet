import {useLayoutEffect} from "react";
import {act, cleanup, renderHook} from "@testing-library/react";
import {afterEach, expect, it, vi} from "vitest";
import {useWakeupControl} from "./use-wakeup-control";

vi.mock("next/navigation", () => ({useRouter: () => ({refresh: vi.fn()})}));
afterEach(cleanup);

it("discards a completed operation after its owner has unmounted", async () => {
  const response = Promise.withResolvers<string>(); const accept = vi.fn();
  const action = vi.fn((_signal: AbortSignal) => response.promise);
  const hook = renderHook(useWakeupControl);
  let operation: Promise<void>;
  act(() => {operation = hook.result.current.run(action, accept);});
  hook.unmount();
  expect(action.mock.calls[0]![0].aborted).toBe(true);
  await act(async () => {response.resolve("saved"); await operation;});
  expect(accept).not.toHaveBeenCalled();
});

it("refuses work before its owner is mounted and after disposal", async () => {
  const action = vi.fn(async () => "saved"); const accept = vi.fn();
  const hook = renderHook(() => {
    const control = useWakeupControl();
    useLayoutEffect(() => {void control.run(action, accept);}, [control]);
    return control;
  });
  expect(action).not.toHaveBeenCalled();
  const run = hook.result.current.run;
  hook.unmount();
  await run(action, accept);
  expect(action).not.toHaveBeenCalled();
  expect(accept).not.toHaveBeenCalled();
});

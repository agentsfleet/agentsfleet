import { afterEach, beforeEach, expect, it } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { renderToString } from "react-dom/server";
import { FleetRunCost } from "./FleetRunCost";
import { FleetStreamProvider } from "./FleetStreamProvider";
import { FleetStreams } from "@/lib/streaming/fleet-stream-registry";
import { FRAME_KIND } from "@/lib/api/events-types";
import { latestFigures } from "@/lib/events/run-summary";
import { mergeBackfill } from "@/lib/streaming/fleet-stream-frames";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { row } from "@/tests/helpers/fleet-stream-fixtures";

const PROPS = {workspaceId: "workspace", fleetId: "fleet"};
const INITIAL = latestFigures(mergeBackfill([], [row({status: "processed", cost_nanos: "340000000"})]));
let streams: FleetStreams;
beforeEach(() => {FakeEventSource.install(); streams = new FleetStreams();});
afterEach(() => {cleanup(); streams.close(); FakeEventSource.uninstall();});

it("shows the reported run cost, leaving unreported costs absent", () => {
  const view = render(<FleetRunCost {...PROPS} />);
  expect(view.container.textContent).toBe("");
  view.rerender(<FleetRunCost {...PROPS} initial={INITIAL} />);
  expect(screen.getByText("$0.34 latest run")).toBeTruthy();
  expect(FakeEventSource.instances).toHaveLength(0);
});

it("renders the server's cost without opening a stream, then updates and clears on revocation", () => {
  expect(renderToString(<FleetStreamProvider streams={streams}><FleetRunCost {...PROPS} initial={INITIAL} /></FleetStreamProvider>)).toContain("$0.34");
  expect(FakeEventSource.instances).toHaveLength(0);
  render(<FleetStreamProvider streams={streams}><FleetRunCost {...PROPS} initial={INITIAL} /></FleetStreamProvider>);
  expect(screen.getByText("$0.34 latest run")).toBeTruthy();
  act(() => streams.reconcileServerRows(PROPS.fleetId, [row({event_id: "new", status: "processed", cost_nanos: "0"})]));
  expect(screen.getByText("$0.00 latest run")).toBeTruthy();
  act(() => FakeEventSource.instances[0]?.emitNamed(FRAME_KIND.ACCESS_REVOKED, JSON.stringify({kind: FRAME_KIND.ACCESS_REVOKED})));
  expect(screen.queryByText(/latest run/)).toBeNull();
  expect(FakeEventSource.instances).toHaveLength(1);
});

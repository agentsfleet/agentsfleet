import { Profiler, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { Button } from "@agentsfleet/design-system";
import { FleetToolOutputDialog } from "../../../components/domain/FleetToolOutputDialog";

export type OutputMeasurement = {startedAt: number; openingMs: number | null; initialRenderMs: number | null; renders: number[]};
declare global {interface Window {outputMeasurement: OutputMeasurement}}

function OutputProbe() {
  const [open, setOpen] = useState(false);
  const frame = useRef<number | undefined>(undefined);
  useEffect(() => () => {if (frame.current !== undefined) cancelAnimationFrame(frame.current);}, []);
  const close = () => {if (frame.current !== undefined) cancelAnimationFrame(frame.current); setOpen(false);};
  const show = () => {
    const measurement: OutputMeasurement = {startedAt: performance.now(), openingMs: null, initialRenderMs: null, renders: []};
    window.outputMeasurement = measurement;
    setOpen(true);
    const observe = () => {
      if (document.querySelector("[data-line-number]")) {
        frame.current = requestAnimationFrame(() => {
          measurement.openingMs = performance.now() - measurement.startedAt;
          measurement.initialRenderMs = Math.max(...measurement.renders);
        });
      } else frame.current = requestAnimationFrame(observe);
    };
    frame.current = requestAnimationFrame(observe);
  };
  return open ? <Profiler id="full-output" onRender={(_id, _stage, actualDuration) => window.outputMeasurement.renders.push(actualDuration)}>
    <FleetToolOutputDialog scope={{workspaceId: "ws_probe", fleetId: "fleet_probe"}} eventId="event_probe" callId="call:1"
      name="http_request" verb="Ran command" target="GET https://example.test/log" outcome={undefined} onClose={close} />
  </Profiler> : <Button onClick={show}>Open output</Button>;
}

const root = document.getElementById("root");
if (!root) throw new Error("Output probe root is missing");
createRoot(root).render(<OutputProbe />);

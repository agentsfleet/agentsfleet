import { __resetRegistryForTests, getSnapshot, subscribe } from "@/lib/streaming/fleet-stream-registry";

// Runs the real registry in Chromium. No EventSource, clock or fetch replacement.
const FLEET = "browser-transport";
const status = document.querySelector("output");
const history = document.querySelector("pre");
const start = document.querySelector("#start");
const stop = document.querySelector("#stop");
if (!status || !history || !start || !stop) throw new Error("Missing probe controls");
const transitions: string[] = [];
const releases: Array<() => void> = [];

function render(): void {
  const current = getSnapshot(FLEET).connectionStatus;
  if (transitions.at(-1) !== current) transitions.push(current);
  if (status) status.textContent = current;
  if (history) history.textContent = JSON.stringify(transitions);
}

start.addEventListener("click", () => {
  // One hundred independent consumers must still share one native transport.
  for (let subscriber = 0; subscriber < 100; subscriber += 1) {
    releases.push(subscribe("browser-workspace", FLEET, [], render));
  }
  render();
});
stop.addEventListener("click", () => {
  releases.splice(0).forEach((release) => release());
  __resetRegistryForTests();
  render();
});

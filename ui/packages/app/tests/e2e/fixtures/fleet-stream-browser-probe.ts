import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { RestartBrowserProbe } from "./restart-browser-probe";
import { __resetRegistryForTests, getSnapshot, subscribe } from "@/tests/helpers/fleet-streams";

// Runs the real registry in Chromium. No EventSource, clock or fetch replacement.
const FLEET = "browser-transport";
const status = document.querySelector("output");
const history = document.querySelector("pre");
const start = document.querySelector("#start");
const stop = document.querySelector("#stop");
if (!status || !history || !start || !stop) throw new Error("Missing probe controls");
const replyHost = document.querySelector("#reply-root");
const reply = replyHost ? createRoot(replyHost) : undefined;
const transitions: string[] = [];
const releases: Array<() => void> = [];

function render(): void {
  const snapshot = getSnapshot(FLEET);
  const current = snapshot.connectionStatus;
  reply?.render(createElement(RestartBrowserProbe, {events: snapshot.events}));
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

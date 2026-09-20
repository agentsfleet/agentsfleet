// The library renders help and parse errors through Console. If that reaches
// the real process streams, a test that injected its own sees an empty buffer
// while the text lands on the terminal running the suite — and `runCli`'s
// promise to write only where it was told is quietly broken.

import { describe, expect, test } from "bun:test";

import { consoleForStreams } from "../src/program/entry/console-bridge.ts";

const sink = (): { lines: string[]; stream: { write(chunk: string): boolean } } => {
  const lines: string[] = [];
  return { lines, stream: { write: (chunk) => { lines.push(chunk); return true; } } };
};

describe("consoleForStreams", () => {
  test("log reaches the stdout it was given, not the process", () => {
    const out = sink();
    const err = sink();
    consoleForStreams(out.stream, err.stream).log("a help line");
    expect(out.lines.join("")).toBe("a help line\n");
    expect(err.lines).toEqual([]);
  });

  test("error reaches the stderr it was given", () => {
    const out = sink();
    const err = sink();
    consoleForStreams(out.stream, err.stream).error("a rejection");
    expect(err.lines.join("")).toBe("a rejection\n");
    expect(out.lines).toEqual([]);
  });

  test("several arguments join the way the real console joins them", () => {
    const out = sink();
    consoleForStreams(out.stream, sink().stream).log("one", "two");
    expect(out.lines.join("")).toBe("one two\n");
  });

  test("a non-string argument is stringified rather than dropped", () => {
    const out = sink();
    consoleForStreams(out.stream, sink().stream).log(42);
    expect(out.lines.join("")).toBe("42\n");
  });

  // Every method the library never calls still has to behave like a console:
  // a partial stand-in that answered `undefined` would turn an unexpected
  // call into a crash rather than a log line.
  test("methods that are not redirected still exist", () => {
    const bridged = consoleForStreams(sink().stream, sink().stream);
    expect(typeof bridged.table).toBe("function");
    expect(typeof bridged.warn).toBe("function");
  });
});

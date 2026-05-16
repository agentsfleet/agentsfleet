// Targeted coverage fills for the `ui` proxy in src/output/index.js (all
// seven members are exported as inline arrow functions, so every member
// has to be invoked to count) and the writeError fallback branch in
// src/program/io.js (the `(s) => s` default ui.err escape hatch).

import { test, expect } from "bun:test";
import { Writable } from "node:stream";

import { ui, printSection, printKeyValue, printTable } from "../src/output/index.ts";
import { writeError } from "../src/program/io.js";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({
      write(chunk, _enc, cb) {
        data += String(chunk);
        cb();
      },
    }),
    read: () => data,
  };
}

test("ui proxy exposes ok / info / warn / err / head / dim / label — every callable returns a string", () => {
  expect(typeof ui.ok("ok")).toBe("string");
  expect(typeof ui.info("info")).toBe("string");
  expect(typeof ui.warn("warn")).toBe("string");
  expect(typeof ui.err("err")).toBe("string");
  expect(typeof ui.head("head")).toBe("string");
  expect(typeof ui.dim("dim")).toBe("string");
  expect(typeof ui.label("label")).toBe("string");
});

test("printSection / printKeyValue / printTable each write to the supplied stream", () => {
  const buf = bufferStream();
  printSection(buf.stream, "title");
  printKeyValue(buf.stream, [["Foo", "bar"]]);
  printTable(buf.stream, [{ label: "Col", key: "k" }], [{ k: "v" }]);
  const out = buf.read();
  expect(out).toContain("title");
  expect(out).toContain("Foo");
  expect(out).toContain("Col");
});

test("writeError without opts.ui uses the inline `(s) => s` fallback for human output", () => {
  const buf = bufferStream();
  const ctx = { jsonMode: false, stderr: buf.stream };
  writeError(ctx, "TEST_CODE", "boom message");
  expect(buf.read()).toContain("boom message");
});

test("writeError in jsonMode emits a JSON envelope to stderr", () => {
  const buf = bufferStream();
  const ctx = { jsonMode: true, stderr: buf.stream };
  writeError(ctx, "TEST_CODE", "boom message");
  const parsed = JSON.parse(buf.read());
  expect(parsed.error.code).toBe("TEST_CODE");
  expect(parsed.error.message).toBe("boom message");
});

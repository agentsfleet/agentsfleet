import {expect, it} from "vitest";
import {libraryErrorFromCause, readErrorFrom} from "./library-types";

it.each([null, undefined, "connection ended"])("contains a non-Error thrown value without failing recovery (%s)", (cause) => {
  expect(libraryErrorFromCause(cause)).toEqual({kind: "unknown", detail: undefined});
});

it("preserves an unexpected transport status as an unknown read failure", () => {
  expect(readErrorFrom({status: 502, error: "upstream unavailable"})).toEqual({kind: "unknown", detail: "upstream unavailable"});
});

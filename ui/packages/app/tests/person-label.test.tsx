import React from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { PersonLabel } from "@/components/domain/PersonLabel";

const SUBJECT = "user_3HizL5hdEfQ9Gy4e6Qsuq9nkKCu";
const SHORTENED = "user_3HizL…kKCu";
const SWEEPER = "system:approval_gate_sweeper";

afterEach(cleanup);

describe("PersonLabel — the name arrives with the row", () => {
  it("prints the name the row carried", () => {
    render(<PersonLabel actor={SUBJECT} name="Ada Lovelace" />);
    expect(screen.getByText("Ada Lovelace")).toBeTruthy();
  });

  // The subject is the identifier of record. A name replaces it on screen and
  // never off it: an operator matching a row against a log line needs the
  // string the log line holds.
  it("keeps the subject reachable as the title", () => {
    render(<PersonLabel actor={SUBJECT} name="Ada Lovelace" />);
    expect(screen.getByText("Ada Lovelace").getAttribute("title")).toBe(SUBJECT);
  });

  // Empty is the ordinary answer for a subject this deployment never saw sign
  // up. Dropping the row's only record of who decided is worse than printing
  // the id, so it renders shortened rather than blank.
  it("falls back to the shortened subject when the join came back empty", () => {
    render(<PersonLabel actor={SUBJECT} name="" />);
    expect(screen.getByText(SHORTENED)).toBeTruthy();
  });

  // The sweeper is not a person and has no user row to join to. Its label is
  // decided here, not by the absence of a name.
  it("names the daemon's own sentinel rather than shortening it", () => {
    render(<PersonLabel actor={SWEEPER} name="" />);
    expect(screen.getByText("Auto-swept")).toBeTruthy();
  });

  it("names an unrecognised sentinel generically", () => {
    render(<PersonLabel actor="system:something_else" name="" />);
    expect(screen.getByText("System")).toBeTruthy();
  });

  // A sentinel is what the daemon calls itself. A name joined against one
  // would be a bug in the join, and the sentinel still wins.
  it("prefers the sentinel over any name a join produced for it", () => {
    render(<PersonLabel actor={SWEEPER} name="Ada Lovelace" />);
    expect(screen.getByText("Auto-swept")).toBeTruthy();
    expect(screen.queryByText("Ada Lovelace")).toBeNull();
  });

  // A pending gate has no decider at all. The column is empty, not "unknown".
  it("renders nothing for an empty actor", () => {
    const { container } = render(<PersonLabel actor="" name="" />);
    expect(container.textContent).toBe("");
  });

  it("passes its className through", () => {
    render(<PersonLabel actor={SUBJECT} name="Ada Lovelace" className="text-xs" />);
    expect(screen.getByText("Ada Lovelace").className).toBe("text-xs");
  });
});

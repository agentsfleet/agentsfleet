/**
 * A card's height stops tracking the worst entry in its row.
 *
 * The gallery is an equal-height grid, so the tallest card sets the height of
 * every card beside it. A seeded entry with a five-sentence description and
 * five credentials gave its five neighbours a screenful of dead space — the
 * defect these tests were written against, measured on the running app.
 *
 * Both bounds are asserted on the RENDERED output rather than on the source,
 * because what has to stay true is what someone sees: three lines and three
 * chips, whatever the entry carries.
 */
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { FleetLibraryGalleryEntry } from "@/lib/types";

import { LibraryCard } from "./LibraryCard";

afterEach(cleanup);

// The entry that caused this: five sentences and five credentials, which is
// four more sentences and four more credentials than its neighbours.
const VERBOSE: FleetLibraryGalleryEntry = {
  id: "incident-responder",
  name: "incident-responder",
  description:
    "Sweeps Grafana and Elastic on a schedule, correlates telemetry with " +
    "recent repository history, and posts an evidence-cited diagnosis to " +
    "Slack and Jira. When the cause is code-shaped it names the suspect " +
    "change and a forward fix, but it cannot carry that fix out — its " +
    "GitHub token is minted read-only, so it reads history and cannot open " +
    "a Pull Request.",
  visibility: "platform",
  source_ref: "agentsfleet/incident-responder",
  requirements: {
    credentials: ["elastic", "grafana", "github", "jira", "slack"],
    tools: [],
    network_hosts: [],
    trigger_present: false,
  },
};

// Its neighbour, which is the shape most of the catalogue already has.
const BRIEF: FleetLibraryGalleryEntry = {
  ...VERBOSE,
  id: "security-reviewer",
  name: "security-reviewer",
  description: "Reviews pull requests for security issues and posts findings.",
  requirements: { ...VERBOSE.requirements, credentials: ["github"] },
};

describe("a gallery card is bounded", () => {
  it("clamps a long description rather than letting it set the row height", () => {
    render(<LibraryCard entry={VERBOSE} action={null} />);

    const description = screen.getByText(/^Sweeps Grafana and Elastic/);
    expect(description.className).toContain("line-clamp-3");
  });

  it("shows three credentials and counts the rest", () => {
    render(<LibraryCard entry={VERBOSE} action={null} />);

    for (const shown of ["elastic", "grafana", "github"]) {
      expect(screen.getByText(`requires: ${shown}`)).toBeTruthy();
    }
    // The two that did not fit are a count, not two more chips — a third row
    // of badges is the other half of what made this card tall.
    expect(screen.queryByText("requires: jira")).toBeNull();
    expect(screen.queryByText("requires: slack")).toBeNull();
    expect(screen.getByText("+2")).toBeTruthy();
  });

  it("names the hidden credentials on the overflow chip", () => {
    // The count answers "roughly what does this need"; hovering answers which,
    // without a card growing to say so. The install dialog is what refuses for
    // a missing one, and it names every credential in full.
    render(<LibraryCard entry={VERBOSE} action={null} />);

    expect(screen.getByText("+2").getAttribute("title")).toBe("jira, slack");
  });

  it("leaves an entry that already fits alone", () => {
    // The clamp is a ceiling, not a reformat: an entry inside both bounds
    // renders exactly what it carries, with no overflow chip invented for it.
    render(<LibraryCard entry={BRIEF} action={null} />);

    expect(screen.getByText("requires: github")).toBeTruthy();
    expect(screen.queryByText(/^\+\d+$/)).toBeNull();
  });

  it("renders no credential row for an entry that needs none", () => {
    render(
      <LibraryCard
        entry={{ ...BRIEF, requirements: { ...BRIEF.requirements, credentials: [] } }}
        action={null}
      />,
    );

    expect(screen.queryByText(/^requires: /)).toBeNull();
    expect(screen.queryByText(/^\+\d+$/)).toBeNull();
  });
});

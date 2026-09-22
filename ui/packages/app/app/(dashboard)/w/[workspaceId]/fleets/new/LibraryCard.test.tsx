/**
 * A card's height stops tracking the worst entry in its row.
 *
 * The gallery is an equal-height grid, so the tallest card sets the height of
 * every card beside it. A seeded entry with a five-sentence description and
 * five credentials gave its five neighbours a screenful of dead space — the
 * defect these tests were written against, measured on the running app.
 *
 * The first fix clamped to three lines and three chips, which straightened the
 * grid and left the reader holding half a sentence: a description cut at "when
 * the cause i…" with the rest a click away in the install dialog, past the
 * decision the card exists to inform. So the card is now four rows that cannot
 * wrap, and everything the clamp hides is on hover instead of on another
 * screen.
 *
 * Asserted on the RENDERED output rather than on the source, because what has
 * to stay true is what someone sees.
 */
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { FleetLibraryGalleryEntry } from "@/lib/types";

import { LibraryCard } from "./LibraryCard";

afterEach(cleanup);

// The card's tooltips come from the design system, which requires a provider.
// The dashboard layout mounts one around every page; these renders stand in
// for it rather than the card carrying its own.
function renderCard(ui: React.ReactNode) {
  return render(<TooltipProvider>{ui}</TooltipProvider>);
}

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
  it("clamps a long description to one line rather than setting the row height", () => {
    renderCard(<LibraryCard entry={VERBOSE} action={null} />);

    const description = screen.getByTestId(`library-card-description-${VERBOSE.id}`);
    expect(description.className).toContain("line-clamp-1");
  });

  it("keeps the whole description in the document, clipped only by the clamp", () => {
    // The clamp is CSS. The sentence a sighted reader loses to it is still
    // here for anyone reading the page another way, and it is what the
    // tooltip repeats — so "the rest is one screen away" stops being true.
    renderCard(<LibraryCard entry={VERBOSE} action={null} />);

    const description = screen.getByTestId(`library-card-description-${VERBOSE.id}`);
    expect(description.textContent).toBe(VERBOSE.description);
    expect(description.textContent).toContain("cannot open a Pull Request");
  });

  it("draws one mark per credential rather than a chip per name", () => {
    // Chips carrying five names wrap to a second line, and a wrapped row is
    // the other half of what made this card tall. The names are on hover.
    renderCard(<LibraryCard entry={VERBOSE} action={null} />);

    const row = screen.getByTestId(`library-card-requires-${VERBOSE.id}`);
    expect(row.querySelectorAll("svg")).toHaveLength(
      VERBOSE.requirements.credentials.length,
    );
    for (const name of VERBOSE.requirements.credentials) {
      expect(screen.queryByText(`Requires: ${name}`)).toBeNull();
    }
  });

  it("draws the marks of the providers it knows", () => {
    renderCard(<LibraryCard entry={VERBOSE} action={null} />);

    const row = screen.getByTestId(`library-card-requires-${VERBOSE.id}`);
    for (const known of ["elastic", "grafana", "github", "jira"]) {
      expect(row.querySelector(`[data-vendor-mark='${known}']`)).toBeTruthy();
    }
    // Slack has no mark upstream and is deliberately not faked into one.
    expect(row.querySelector("[data-vendor-mark='slack']")).toBeNull();
  });

  it("caps the marks and counts the rest", () => {
    // Six is past the ceiling, and the row still has to be one line. The chip
    // says how many are not drawn; the tooltip names every one of them,
    // drawn or not.
    const CROWDED = {
      ...VERBOSE,
      id: "crowded",
      requirements: {
        ...VERBOSE.requirements,
        credentials: ["elastic", "grafana", "github", "jira", "slack", "zoho"],
      },
    };
    renderCard(<LibraryCard entry={CROWDED} action={null} />);

    const row = screen.getByTestId("library-card-requires-crowded");
    expect(row.querySelectorAll("svg")).toHaveLength(5);
    // `+` and the count are separate text nodes, so this reads the chip whole.
    expect(row.textContent).toContain("+1");
    // The sixth is not drawn, so its mark must not be either.
    expect(row.querySelector("[data-vendor-mark='zoho']")).toBeNull();
  });

  it("renders no credential row for an entry that needs none", () => {
    renderCard(
      <LibraryCard
        entry={{ ...BRIEF, requirements: { ...BRIEF.requirements, credentials: [] } }}
        action={null}
      />,
    );

    expect(screen.queryByTestId(`library-card-requires-${BRIEF.id}`)).toBeNull();
  });
});

// Two entries of the SAME name across the two tiers is the case the gallery
// actually serves — a workspace onboards its own copy of a bundle the
// platform also publishes — and it is the case that was indistinguishable.
describe("a card names the catalogue its entry came from", () => {
  const PLATFORM_COPY: FleetLibraryGalleryEntry = {
    ...BRIEF,
    id: "github-pr-reviewer-platform",
    name: "github-pr-reviewer",
    visibility: "platform",
  };
  const WORKSPACE_COPY: FleetLibraryGalleryEntry = {
    ...PLATFORM_COPY,
    id: "github-pr-reviewer-tenant",
    visibility: "tenant",
  };

  it("names the platform tier on the mark, not in the wire's spelling", () => {
    renderCard(<LibraryCard entry={PLATFORM_COPY} action={null} />);

    const tier = screen.getByTestId("library-card-tier-github-pr-reviewer-platform");
    expect(tier.textContent).toBe("Platform catalogue");
  });

  it("names a workspace's own copy as the workspace's", () => {
    renderCard(<LibraryCard entry={WORKSPACE_COPY} action={null} />);

    const tier = screen.getByTestId("library-card-tier-github-pr-reviewer-tenant");
    expect(tier.textContent).toBe("This workspace");
  });

  it("two same-named entries are distinguishable by accessible name", () => {
    // The tier is a mark now, so "distinguishable by rendered text" is no
    // longer the property — the word moved to the accessible name and the
    // tooltip. That is the thing to hold: a person who cannot hover, or who
    // is not looking, must still be able to tell the two apart, because
    // installing the wrong one of two identically named entries is silent.
    renderCard(
      <>
        <LibraryCard entry={PLATFORM_COPY} action={null} />
        <LibraryCard entry={WORKSPACE_COPY} action={null} />
      </>,
    );

    expect(screen.getAllByText("github-pr-reviewer")).toHaveLength(2);
    expect(screen.getByText("Platform catalogue")).toBeTruthy();
    expect(screen.getByText("This workspace")).toBeTruthy();
  });
});

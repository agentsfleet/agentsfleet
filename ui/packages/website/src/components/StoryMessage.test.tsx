import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { DEFAULT_JOB } from "../lib/example-jobs";
import { STORY_DETAIL, STORY_ROLE } from "../lib/chat-story";
import { StoryMessage } from "./StoryMessage";

const CONTEXT = "  Worker environment";
const REMOVED = "- BROWSER_TIMEOUT=5";
const ADDED = "+ BROWSER_TIMEOUT=10";
const PROPOSAL_LABEL = "Proposed change, not applied";
const DIFF_ADD = "diff-add";
const DIFF_REMOVE = "diff-remove";

it("distinguishes removed lines, added lines, and unchanged proposal context", () => {
  render(<StoryMessage job={{ ...DEFAULT_JOB, code: [CONTEXT, REMOVED, ADDED] }}
    entry={{ step: { role: STORY_ROLE.fleet, text: DEFAULT_JOB.proposal, detail: STORY_DETAIL.proposal }, text: DEFAULT_JOB.proposal, complete: true }}
    onEvidence={() => { throw new Error("A proposal must not select evidence"); }} />);
  const proposal = screen.getByLabelText(PROPOSAL_LABEL);
  expect(proposal.textContent).toBe([CONTEXT, REMOVED, ADDED, ""].join("\n"));
  expect(screen.getByText(REMOVED)).toHaveClass(DIFF_REMOVE);
  expect(screen.getByText(ADDED)).toHaveClass(DIFF_ADD);
  expect(screen.getByText(CONTEXT.trim())).not.toHaveClass(DIFF_ADD, DIFF_REMOVE);
});

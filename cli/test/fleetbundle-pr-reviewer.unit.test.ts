// The shipped reviewer bundle must address the Pull Request the EVENT names.
//
// This fixture is not parser input; it is the prose a model executes. A bundle
// that says "read the pull request diff" without naming where the repository
// and the number come from leaves the model to guess, and a guess is wrong on
// the second delivery and every one after it. The live fleet shipped exactly
// that prose and reviewed nothing.
//
// Asserted on the file rather than on a parse, because the defect is in what
// the sentences say — a bundle can be perfectly well-formed and still tell a
// model nothing it can act on.

import { describe, test, expect } from "bun:test";

const BUNDLE_DIR = "tests/fixtures/fleetbundle/github-pr-reviewer";

/// The two payload fields that address a Pull Request, spelled as the GitHub
/// `pull_request` event spells them.
const EVENT_FIELDS = ["repository.full_name", "pull_request.number"] as const;

const skill = async () =>
  await Bun.file(`${import.meta.dir}/../../${BUNDLE_DIR}/SKILL.md`).text();

describe("github-pr-reviewer bundle — the event names the Pull Request", () => {
  test("test_m202_001_fixture_skill_reads_the_event", async () => {
    const text = await skill();

    for (const field of EVENT_FIELDS) {
      expect(text).toContain(field);
    }
  });

  test("test_m202_001_fixture_skill_reads_the_event — no number is hard-coded", async () => {
    const text = await skill();
    // Any `/pulls/<digits>` is a number the bundle chose rather than read.
    // The templated form `/pulls/{pull_request.number}` is what should appear,
    // and it does not match this pattern.
    const hardCoded = text.match(/\/pulls\/\d+/g);

    expect(hardCoded).toBeNull();
  });

  test("test_m202_001_fixture_skill_reads_the_event — comment only survives", async () => {
    // The constraint that makes a reviewer safe to auto-authorise at all. The
    // grant admits the mint without asking per event, so what keeps this fleet
    // to comments is the bundle's own instruction plus the token's scope.
    const text = await skill();

    expect(text).toContain("never push, merge, approve, or close");
    expect(text).toContain("COMMENT");
  });
});

// The scenario page is the end-to-end walk this milestone made true. Asserted
// here because the spec template gate requires a tiered test per Dimension —
// leaving the check to REVIEW alone is not a shape the harness accepts.
//
// Kept narrow on purpose: a page that re-documents a retired per-event card
// would be describing a decision the daemon can no longer make, and that is the
// only failure worth failing a build over. Wording is otherwise free to change.
describe("scenario page — one authorisation, not two", () => {
  const scenario = async () =>
    await Bun.file(
      `${import.meta.dir}/../../docs/architecture/scenarios/github-pr-reviewer.md`,
    ).text();

  test("test_m202_001_scenario_page_names_one_authorisation", async () => {
    const text = await scenario();

    expect(text).toContain("core.integration_grants");
    expect(text).toContain("raises no approval card");
    // The retired kind may be NAMED as history — the page explains why it went
    // — but never as a step the reader should expect to perform.
    expect(text).not.toContain("approve the repository_write card");
  });
});

---
name: orly-spec
description: |
  Turn an idea into a reviewed spec under docs/v{N}/pending/: brainstorm, review
  and resolve with the Elephant-Goldfish Model (EGM), draft from the spec
  standard, and land it through a docs-only Pull Request. Use on "spec this",
  "create a spec", "new milestone", "spec out X", or any TODO.md attempt.
---

# orly-spec

A spec is the rulebook the building agent ships from. This skill owns the steps
and writes no code. The rules a spec must meet live in `.orly/standards/spec.md`,
and its shape in `.orly/standards/spec-template.md`.

## Steps

1. **Check the setup.** Run `orly doctor`. Stop on any red line and show it:
   the skill needs the repository's rules installed and gstack and EGM present.
2. **Explore.** Restate the idea in your own words. Find prior art in this
   repository and its siblings under `~/Projects`, then run EGM's
   `egm-brainstorm` on the idea.
3. **Choose.** Put two to four shapes with their costs to the owner through
   AskUserQuestion, with your call first. A question that times out is not an
   answer: wait. Record the owner's verbatim words in Discovery.
4. **Name.** Fetch `origin/main`, then take the next free `M{N}_{WS}` from
   `git ls-tree -r --name-only origin/main docs/`. The file is
   `docs/v{P}/pending/M{N}_{WS}_P{priority}_{CATEGORIES}_{NAME}.md`.
5. **Draft.** Copy the spec template and fill it in the order the spec standard
   gives. Delete every `tpl:` hint. Every Dimension names its test, and every
   failure mode its negative test.
6. **Review.** Run EGM's `egm-review` on the draft. Its reviewers start with no
   context. Apply accepted findings with `egm-resolve`; scope and design calls
   go to the owner.
7. **Land.** Cut `docs/m{N}-<slug>-spec` from the fetched `origin/main`,
   commit the spec, push, and open a docs-only Pull Request. The default
   branch requires pull requests.

## Stop conditions

- `orly doctor` is red, or EGM is missing: report the doctor lines.
- The goal cannot be written as a test name: the intent is not understood yet.
- A `[?]` survives the golden-path walk: the spec stays in `pending/`.

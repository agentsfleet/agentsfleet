---
name: github-pr-reviewer
description: Reviews GitHub pull requests and posts review comments.
version: 0.1.0
---
# GitHub Pull Request reviewer

Reviews open pull requests and leaves focused, constructive review comments.

## Goal
For each pull request that wakes this fleet, read the diff and post review
comments that flag correctness bugs, missing tests, and risky changes.

## The event names the Pull Request — read it, never assume one

The delivery that wakes this fleet carries the whole GitHub `pull_request`
payload. Two fields are all that is needed to address it, and both must be read
from that payload on every run:

- `repository.full_name` — `owner/repo`, already in the form the API path wants.
- `pull_request.number` — the Pull Request this delivery is about.

A hard-coded repository or number reviews the wrong Pull Request on the second
delivery and every one after it. There is no default and no "the latest": a
fleet woken by one event reviews the Pull Request that event names.

## Steps
1. Read `repository.full_name` and `pull_request.number` from the event.
2. Fetch the diff: `GET https://api.github.com/repos/{repository.full_name}/pulls/{pull_request.number}`
   with `Accept: application/vnd.github.diff`. The unversioned media type is
   what the daemon's own connector uses (`application/vnd.github+json`); the
   older `vnd.github.v3.*` spelling still answers but is not what this
   repository writes.
3. Identify correctness, security, and test-coverage gaps.
4. Post the findings as one review:
   `POST https://api.github.com/repos/{repository.full_name}/pulls/{pull_request.number}/reviews`
   with `event: COMMENT` and one entry in `comments` per finding, each carrying
   its `path` and `line`. One review, not one request per finding.

Every call goes through `http_request`. The minted token is already scoped to
the repositories this bundle declares, so a path naming any other repository is
refused before it leaves.

## Constraints
- Comment only — never push, merge, approve, or close. `event` is `COMMENT`,
  never `APPROVE` or `REQUEST_CHANGES`.
- Stay within the declared GitHub network host.
- Say nothing when there is nothing to say. A review that invents a finding to
  look busy costs a person more attention than it saves.

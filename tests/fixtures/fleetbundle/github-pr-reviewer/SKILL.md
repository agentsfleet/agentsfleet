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

The delivery that wakes this fleet is NOT GitHub's raw webhook. The daemon
reduces it to a flat digest and that digest is the event body, so the raw
payload's nested paths do not exist here. Two of its fields address the Pull
Request, and both must be read on every run:

- `repo` — `owner/repo`, already in the form the API path wants.
- `number` — the Pull Request this delivery is about.

The digest also carries `action`, `title`, `url`, `state`, `draft`, `author`,
`head_ref` and `base_ref`. Read those instead of fetching them again.

A hard-coded repository or number reviews the wrong Pull Request on the second
delivery and every one after it. There is no default and no "the latest": a
fleet woken by one event reviews the Pull Request that event names.

## Steps
1. Read `repo` and `number` from the event.
2. Fetch the diff: `GET https://api.github.com/repos/{repo}/pulls/{number}`
   with `Accept: application/vnd.github.diff`. The unversioned media type is
   what the daemon's own connector uses (`application/vnd.github+json`); the
   older `vnd.github.v3.*` spelling still answers but is not what this
   repository writes.
3. Identify correctness, security, and test-coverage gaps.
4. Post the findings as one review:
   `POST https://api.github.com/repos/{repo}/pulls/{number}/reviews`
   with `event: COMMENT` and one entry in `comments` per finding, each carrying
   its `path` and `line`. One review, not one request per finding.

Every call goes through `http_request`. The minted token is already scoped to
the repositories this bundle declares, so a path naming any other repository is
refused before it leaves.

## Operator steers and memory

An operator steer without a pull request event is a chat request. Do not fetch
a diff or post a GitHub review for that steer.

When an operator steer adds a lasting preference, a fact to remember, or task
progress, call `memory_store` before answering. Use a stable key such as
`operator_context:codename` or `operator_context:review_status`, category
`core`, and concise content. Reuse the same key to update a fact. Confirm it
was saved only after the tool succeeds. Do not save credentials or transcripts.

When asked about earlier steers, call `memory_recall` with query
`operator_context` before answering. Use the returned facts; if recall finds
nothing, say so instead of guessing.

## Constraints
- Comment only — never push, merge, approve, or close. `event` is `COMMENT`,
  never `APPROVE` or `REQUEST_CHANGES`.
- Stay within the declared GitHub network host.
- Say nothing when there is nothing to say. A review that invents a finding to
  look busy costs a person more attention than it saves.

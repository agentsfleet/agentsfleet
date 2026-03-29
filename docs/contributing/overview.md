# Contributing

UseZombie is open source under the [MIT License](https://github.com/usezombie/usezombie/blob/main/LICENSE).

## What we welcome

- Bug reports with reproduction steps.
- Feature requests with a clear use case.
- Documentation improvements.
- Performance optimizations with benchmarks.
- New gate implementations for the pipeline.
- CLI improvements and new commands.

## Pull request process

1. Fork the repository.
2. Create a feature branch from `main`.
3. Make your changes. Follow the existing code style.
4. Run `make lint` and `make test` -- both must pass.
5. Open a PR against `main` with a clear description of what and why.

## Code of conduct

We follow the [Contributor Covenant Code of Conduct](https://www.contributor-covenant.org/version/2/1/code_of_conduct/). Be respectful, constructive, and inclusive.

## Commit style

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(pipeline): add retry logic for transient gate failures
fix(cli): correct workspace list output alignment
docs(api): update error code reference
```

## Use cases

UseZombie serves several distinct audiences and workflows:

1. **Solo Builder** -- One developer submits specs and reviews PRs. Everything between spec and PR is autonomous.
2. **Small Team** -- Engineers queue specs into a backlog-to-PR pipeline with scored PRs, cost control, and clear operational boundaries.
3. **Agent-To-Agent** -- An external planner agent writes specs and submits them via the API. UseZombie returns validated PRs with scorecards.
4. **Rollout / Upgrade** -- Operators upgrade worker or executor binaries using drain-then-restart semantics. In-flight work is retried from persisted state.
5. **Free Plan Exhaustion** -- Solo builders start with $10 free credit. Once exhausted, the API rejects new runs with a clear upgrade path.
6. **Workspace Operator Controls** -- Operators manage harnesses, skill secrets, and scoring configuration via RBAC-scoped endpoints (user / operator / admin).
7. **Scored Agent Selection (Phase 2)** -- Multiple agent profiles compete on the same spec. The highest-scoring profile opens the PR; score history tracks quality over time.

## Website content

The website content map defines the marketing copy, feature cards, pricing tiers, and messaging guardrails for `ui/packages/website/`. See [`docs/contributing/website-content.md`](website-content.md) for the full content source.

## Questions

Open a GitHub Discussion or reach out on Discord for questions that are not bug reports or feature requests.

# JavaScript CLI

TypeScript + Bun runtime. `commander` for command parsing, `zod` for validation.

## Stack Inputs

- CLI name + one-line purpose
- Primary mode (`human`, `script`, or `both`)

## Required Files (additional)

- `package.json`
- `src/cli.ts` (entrypoint)
- `src/commands/*` (subcommands)
- `src/lib/config.ts`
- `README.md` with usage and examples

## Command Contract

- Always support `-h/--help` and `--version`.
- stdout for primary output, stderr for diagnostics.
- `--json` for machine output when automation-facing.
- Destructive actions require `--dry-run` and confirmation/`--force`.

## Quality Commands

```bash
bun run lint
bun run typecheck
bun test
bun run build
```

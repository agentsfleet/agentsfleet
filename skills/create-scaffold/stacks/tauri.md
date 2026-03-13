# Tauri Desktop

Tauri 2 + Rust backend + TypeScript frontend. Electron is not the default.

## Stack Inputs

- Frontend framework (`react` by default, unless existing repo says otherwise)
- Signing/release target (`dev` only vs distributable build)

## Required Files (additional)

- `src-tauri/Cargo.toml`
- `src-tauri/src/main.rs`
- `src-tauri/tauri.conf.json`
- Frontend app source (`src/`)
- `package.json`

## Quality Commands

```bash
bun run lint
bun run typecheck
bun test
bunx tauri build --debug
```

## UI Contract

- Use `skills/frontend-design/SKILL.md` when designing new desktop webview surfaces.
- If existing Angular/Material UI is present, preserve current design system.

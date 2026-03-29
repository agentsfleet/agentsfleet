# Development setup

## Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| Zig | Latest stable | zombied server and worker |
| Node.js | 18+ | zombiectl CLI |
| Docker | Latest | Postgres and Redis for local dev |
| Make | Any | Task runner for all components |

## Start services

Bring up Postgres and Redis in Docker:

```bash
make up
```

## Run zombied in dev mode

```bash
make dev
```

This starts the server with hot reload, connected to the local Postgres and Redis instances.

## Lint

```bash
make lint
```

Runs all linters and type checks across the full codebase (Zig, TypeScript, and UI).

## Test

```bash
make test
```

Runs the full unit test suite. Requires no external services.

## Build

```bash
make build
```

Compiles production binaries for zombied and runs the TypeScript build for zombiectl.

## zombiectl development

The CLI lives in its own directory with a separate dependency tree:

```bash
cd zombiectl
npm install
npm run dev
```

## Stop services

```bash
make down
```

Stops and removes the Docker containers started by `make up`.

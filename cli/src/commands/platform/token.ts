import type { CliError } from "../../errors/index.ts";
import { closeSync, fchmodSync, fsyncSync, openSync, unlinkSync, writeFileSync } from "node:fs";
import { Effect } from "effect";
import { UnexpectedError, ValidationError } from "../../errors/index.ts";

type TokenFile = {readonly descriptor: number; readonly path: string; written: boolean};
const FILE_DETAIL = "Cannot create the token file; use a new path in a private, writable directory";
const FILE_HINT = "pass --token-out <file>; an existing file is never overwritten";
const WRITE_DETAIL = "The server issued a token, but saving it failed";
const WRITE_HINT = "rotate this runner's token into a new writable file; do not repeat enrollment";

/** Reserve the destination before minting a credential; failures never expose its bytes. */
export function withRunnerTokenFile<A, R>(path: string, use: (save: (token: string) => Effect.Effect<void, UnexpectedError>) => Effect.Effect<A, CliError, R>) {
  return Effect.acquireUseRelease(
    Effect.try({try: () => openTokenFile(path), catch: () => new ValidationError({detail: FILE_DETAIL, suggestion: FILE_HINT})}),
    (file) => use((token) => Effect.try({
      try: () => {writeFileSync(file.descriptor, `${token}\n`); fsyncSync(file.descriptor); file.written = true;},
      catch: () => new UnexpectedError({detail: WRITE_DETAIL, suggestion: WRITE_HINT}),
    })),
    (file) => Effect.sync(() => {closeSync(file.descriptor); if (!file.written) unlinkSync(file.path);}),
  );
}

function openTokenFile(path: string): TokenFile {
  const descriptor = openSync(path, "wx", 0o600);
  try {fchmodSync(descriptor, 0o600); return {descriptor, path, written: false};}
  catch (cause) {closeSync(descriptor); unlinkSync(path); throw cause;}
}

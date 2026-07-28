import { createHash } from "node:crypto";
import {
  mkdir,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { dirname, relative, resolve } from "node:path";

const TEXT_ENCODING = "utf8";
const FINGERPRINT_ALGORITHM = "sha256";
const HASH_FIELD_SEPARATOR = "\0";
const PACKAGE_MANIFEST = "package.json";
const TYPESCRIPT_CONFIG = "tsconfig.json";
const PROVENANCE_SCHEMA_VERSION = 1;
const PROVENANCE_FILE = ".next/agentsfleet-route-build-provenance.json";
const BUILD_ENVIRONMENT_FIELDS = [
  "NEXT_PUBLIC_API_URL",
  "NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY",
  "NEXT_PUBLIC_CLERK_SIGN_IN_URL",
  "NEXT_PUBLIC_CLERK_SIGN_UP_URL",
  "NEXT_PUBLIC_POSTHOG_ENABLED",
  "NEXT_PUBLIC_POSTHOG_HOST",
  "NEXT_PUBLIC_POSTHOG_KEY",
] as const;

type BuildProvenance = {
  schema_version: typeof PROVENANCE_SCHEMA_VERSION;
  fingerprint: string;
};

type BuildEnvironment = Record<string, string | undefined>;

const buildEnvironment: BuildEnvironment = process.env;

async function filesBelow(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const paths = await Promise.all(
    entries.map(async (entry) => {
      const path = resolve(directory, entry.name);
      if (entry.isDirectory()) return filesBelow(path);
      return entry.isFile() ? [path] : [];
    }),
  );
  return paths.flat();
}

export async function routeBuildInputPaths(
  appRoot: string,
): Promise<string[]> {
  const repositoryRoot = resolve(appRoot, "../../..");
  const designSystemRoot = resolve(appRoot, "../design-system");
  const sourceRoots = [
    "app",
    "components",
    "hooks",
    "lib",
  ].map((directory) => resolve(appRoot, directory));
  const sourceFiles = (
    await Promise.all([
      ...sourceRoots.map(filesBelow),
      filesBelow(resolve(designSystemRoot, "src")),
    ])
  ).flat();
  return [
    ...sourceFiles.filter(
      (path) =>
        !path.includes(".test.") &&
        !path.includes(".spec."),
    ),
    resolve(repositoryRoot, "bun.lock"),
    resolve(appRoot, "global.d.ts"),
    resolve(appRoot, "instrumentation-client.ts"),
    resolve(appRoot, "next.config.ts"),
    resolve(appRoot, PACKAGE_MANIFEST),
    resolve(appRoot, "postcss.config.mjs"),
    resolve(appRoot, "proxy.ts"),
    resolve(appRoot, TYPESCRIPT_CONFIG),
    resolve(designSystemRoot, PACKAGE_MANIFEST),
    resolve(designSystemRoot, TYPESCRIPT_CONFIG),
  ].sort();
}

export async function createBuildFingerprint(
  basePath: string,
  inputPaths: string[],
  environment: BuildEnvironment = buildEnvironment,
): Promise<string> {
  const hash = createHash(FINGERPRINT_ALGORITHM);
  for (const path of [...inputPaths].sort()) {
    hash.update(relative(basePath, path));
    hash.update(HASH_FIELD_SEPARATOR);
    hash.update(await readFile(path));
    hash.update(HASH_FIELD_SEPARATOR);
  }
  for (const field of BUILD_ENVIRONMENT_FIELDS) {
    hash.update(field);
    hash.update(HASH_FIELD_SEPARATOR);
    hash.update(environment[field] ?? "");
    hash.update(HASH_FIELD_SEPARATOR);
  }
  return hash.digest("hex");
}

async function currentProvenance(
  appRoot: string,
  inputPaths: string[] | undefined,
  environment: BuildEnvironment,
): Promise<BuildProvenance> {
  const paths = inputPaths ?? (await routeBuildInputPaths(appRoot));
  return {
    schema_version: PROVENANCE_SCHEMA_VERSION,
    fingerprint: await createBuildFingerprint(appRoot, paths, environment),
  };
}

export async function captureRouteBuildFingerprint(
  appRoot: string,
  inputPaths?: string[],
  environment: BuildEnvironment = buildEnvironment,
): Promise<string> {
  const path = resolve(appRoot, PROVENANCE_FILE);
  await rm(path, { force: true });
  return (await currentProvenance(appRoot, inputPaths, environment)).fingerprint;
}

export async function writeRouteBuildProvenance(
  appRoot: string,
  expectedFingerprint: string,
  inputPaths?: string[],
  environment: BuildEnvironment = buildEnvironment,
): Promise<void> {
  const path = resolve(appRoot, PROVENANCE_FILE);
  const provenance = await currentProvenance(
    appRoot,
    inputPaths,
    environment,
  );
  if (provenance.fingerprint !== expectedFingerprint) {
    throw new Error(
      "route build inputs changed while building; run bun run build again",
    );
  }
  await mkdir(dirname(path), { recursive: true });
  await writeFile(
    path,
    `${JSON.stringify(provenance, null, 2)}\n`,
    TEXT_ENCODING,
  );
}

export async function assertRouteBuildFresh(
  appRoot: string,
  inputPaths?: string[],
  environment: BuildEnvironment = buildEnvironment,
): Promise<void> {
  const path = resolve(appRoot, PROVENANCE_FILE);
  let saved: unknown;
  try {
    saved = JSON.parse(await readFile(path, TEXT_ENCODING));
  } catch {
    throw new Error("route build provenance is absent or invalid; run bun run build");
  }
  const current = await currentProvenance(
    appRoot,
    inputPaths,
    environment,
  );
  if (
    typeof saved !== "object" ||
    saved === null ||
    !("schema_version" in saved) ||
    !("fingerprint" in saved) ||
    saved.schema_version !== current.schema_version ||
    saved.fingerprint !== current.fingerprint
  ) {
    throw new Error("route build output is stale; run bun run build");
  }
}

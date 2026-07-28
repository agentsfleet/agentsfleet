import {
  mkdir,
  mkdtemp,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { relative, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  assertRouteBuildFresh,
  captureRouteBuildFingerprint,
  createBuildFingerprint,
  routeBuildInputPaths,
  writeRouteBuildProvenance,
} from "./route-build-provenance";

const TEXT_ENCODING = "utf8";
const temporaryRoots: string[] = [];

async function temporaryApp() {
  const root = await mkdtemp(resolve(tmpdir(), "agentsfleet-route-build-"));
  temporaryRoots.push(root);
  const appRoot = resolve(root, "ui/packages/app");
  await mkdir(appRoot, { recursive: true });
  const first = resolve(appRoot, "first.ts");
  const second = resolve(appRoot, "second.ts");
  await writeFile(first, "export const first = 1;\n", TEXT_ENCODING);
  await writeFile(second, "export const second = 2;\n", TEXT_ENCODING);
  return { appRoot, first, second };
}

async function completeAppInputs() {
  const { appRoot } = await temporaryApp();
  const repositoryRoot = resolve(appRoot, "../../..");
  const designSystemRoot = resolve(appRoot, "../design-system");
  const sourceDirectories = [
    resolve(appRoot, "app/nested"),
    resolve(appRoot, "components"),
    resolve(appRoot, "hooks"),
    resolve(appRoot, "lib"),
    resolve(designSystemRoot, "src"),
  ];
  await Promise.all(
    sourceDirectories.map((path) => mkdir(path, { recursive: true })),
  );
  const files = [
    resolve(repositoryRoot, "bun.lock"),
    resolve(appRoot, "app/nested/page.tsx"),
    resolve(appRoot, "components/Shell.tsx"),
    resolve(appRoot, "hooks/use-live.ts"),
    resolve(appRoot, "lib/client.ts"),
    resolve(appRoot, "global.d.ts"),
    resolve(appRoot, "instrumentation-client.ts"),
    resolve(appRoot, "next.config.ts"),
    resolve(appRoot, "package.json"),
    resolve(appRoot, "postcss.config.mjs"),
    resolve(appRoot, "proxy.ts"),
    resolve(appRoot, "tsconfig.json"),
    resolve(designSystemRoot, "package.json"),
    resolve(designSystemRoot, "src/Button.tsx"),
    resolve(designSystemRoot, "tsconfig.json"),
  ];
  await Promise.all(
    files.map((path) => writeFile(path, `${relative(appRoot, path)}\n`)),
  );
  const excludedTest = resolve(appRoot, "components/Shell.test.tsx");
  await writeFile(excludedTest, "excluded\n");
  await symlink(
    resolve(appRoot, "lib/client.ts"),
    resolve(appRoot, "components/client-link.ts"),
  );
  return { appRoot, excludedTest, files };
}

afterEach(async () => {
  await Promise.all(
    temporaryRoots.splice(0).map((path) =>
      rm(path, { recursive: true, force: true }),
    ),
  );
});

describe("route build provenance", () => {
  it("is deterministic across input order and changes with source bytes", async () => {
    const { appRoot, first, second } = await temporaryApp();
    const environment = { NEXT_PUBLIC_API_URL: "https://api.example.test" };
    const initial = await createBuildFingerprint(
      appRoot,
      [first, second],
      environment,
    );
    expect(
      await createBuildFingerprint(
        appRoot,
        [second, first],
        environment,
      ),
    ).toBe(initial);

    await writeFile(first, "export const first = 3;\n", TEXT_ENCODING);
    expect(
      await createBuildFingerprint(
        appRoot,
        [first, second],
        environment,
      ),
    ).not.toBe(initial);
  });

  it("includes production inputs and named public build settings", async () => {
    const { appRoot, excludedTest, files } = await completeAppInputs();
    const paths = await routeBuildInputPaths(appRoot);
    expect(paths).toEqual([...files].sort());
    expect(paths).not.toContain(excludedTest);

    const initial = await createBuildFingerprint(appRoot, paths, {
      NEXT_PUBLIC_API_URL: "https://api-one.example.test",
    });
    const changed = await createBuildFingerprint(appRoot, paths, {
      NEXT_PUBLIC_API_URL: "https://api-two.example.test",
    });
    expect(changed).not.toBe(initial);
    await writeRouteBuildProvenance(appRoot, initial, paths, {
      NEXT_PUBLIC_API_URL: "https://api-one.example.test",
    });
    await expect(
      assertRouteBuildFresh(appRoot, paths, {
        NEXT_PUBLIC_API_URL: "https://api-two.example.test",
      }),
    ).rejects.toThrow("stale");
    expect(await createBuildFingerprint(appRoot, paths)).toHaveLength(64);

    const captured = await captureRouteBuildFingerprint(appRoot);
    await writeRouteBuildProvenance(appRoot, captured);
    await expect(assertRouteBuildFresh(appRoot)).resolves.toBeUndefined();
  });

  it("rejects absent and stale build output", async () => {
    const { appRoot, first, second } = await temporaryApp();
    const inputs = [first, second];
    await expect(
      assertRouteBuildFresh(appRoot, inputs),
    ).rejects.toThrow("absent or invalid");

    const fingerprint = await captureRouteBuildFingerprint(appRoot, inputs);
    await writeRouteBuildProvenance(appRoot, fingerprint, inputs);
    await expect(
      assertRouteBuildFresh(appRoot, inputs),
    ).resolves.toBeUndefined();

    await writeFile(second, "export const second = 4;\n", TEXT_ENCODING);
    await expect(
      assertRouteBuildFresh(appRoot, inputs),
    ).rejects.toThrow("stale");
  });

  it("does not publish provenance when inputs change during the build", async () => {
    const { appRoot, first, second } = await temporaryApp();
    const inputs = [first, second];
    const fingerprint = await captureRouteBuildFingerprint(appRoot, inputs);
    await writeFile(first, "export const first = 5;\n", TEXT_ENCODING);
    await expect(
      writeRouteBuildProvenance(appRoot, fingerprint, inputs),
    ).rejects.toThrow("inputs changed while building");
    await expect(
      assertRouteBuildFresh(appRoot, inputs),
    ).rejects.toThrow("absent or invalid");
  });
});

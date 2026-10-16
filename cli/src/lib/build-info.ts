import { readFile } from "node:fs/promises";
const BUILD_ID_FIELD = "build_id";

/** Read the build-time stamp, never a value supplied by the invoking shell. */
export async function readBuildInfo(): Promise<{ build_id: string | null }> {
  // The published command also runs under Node, so this module uses its filesystem API.
  const raw = await readFile(new URL("../build-info.json", import.meta.url), "utf8");
  const value: unknown = JSON.parse(raw);
  if (typeof value !== "object" || value === null || !Object.hasOwn(value, BUILD_ID_FIELD)) {
    throw new Error("CLI build identity is unreadable; rebuild the command");
  }
  const id: unknown = Reflect.get(value, BUILD_ID_FIELD);
  if (id !== null && (typeof id !== "string" || !/^[0-9a-f]{64}$/.test(id))) {
    throw new Error("CLI build identity is invalid; rebuild the command");
  }
  return { build_id: id };
}

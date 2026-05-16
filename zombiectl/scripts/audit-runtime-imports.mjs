#!/usr/bin/env node

import { builtinModules } from "node:module";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { extname, join, resolve } from "node:path";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const PACKAGE_JSON = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));
const RUNTIME_DEPS = new Set(Object.keys(PACKAGE_JSON.dependencies ?? {}));
const DEV_DEPS = new Set(Object.keys(PACKAGE_JSON.devDependencies ?? {}));
const BUILTINS = new Set([...builtinModules, ...builtinModules.map((name) => `node:${name}`)]);
const SOURCE_DIRS = ["src", "bin"];
const IMPORT_PATTERN = /(?:import\s+(?:[^'"]*?\s+from\s+)?|import\s*\()\s*["']([^"']+)["']/g;

const failures = [];

for (const file of listJavaScriptFiles(SOURCE_DIRS.map((dir) => join(ROOT, dir)))) {
  const source = readFileSync(file, "utf8");
  for (const specifier of importSpecifiers(source)) {
    auditSpecifier(file, specifier);
  }
}

if (failures.length > 0) {
  console.error("Runtime import audit failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("Runtime import audit passed");

function listJavaScriptFiles(paths) {
  const files = [];
  for (const path of paths) walk(path, files);
  return files;
}

function walk(path, files) {
  const stat = statSync(path);
  if (stat.isFile()) {
    // Walk both .js and .ts during the TypeScript migration. After the
    // migration completes and the build emits .js to dist/, the published
    // artifact will be pure .js — at that point this audit can be narrowed
    // back to scan the emit output only.
    if (path.endsWith(".js") || path.endsWith(".mjs") || path.endsWith(".ts")) files.push(path);
    return;
  }
  for (const entry of readdirSync(path)) {
    if (entry === "node_modules") continue;
    walk(join(path, entry), files);
  }
}

function importSpecifiers(source) {
  const specifiers = [];
  for (const match of source.matchAll(IMPORT_PATTERN)) specifiers.push(match[1]);
  return specifiers;
}

function auditSpecifier(file, specifier) {
  if (specifier.startsWith(".") || specifier.startsWith("/")) {
    auditRelativeSpecifier(file, specifier);
    return;
  }
  auditPackageSpecifier(file, specifier);
}

function auditRelativeSpecifier(file, specifier) {
  const extension = extname(specifier);
  // `.ts` accepted during the TypeScript migration. tsc's
  // `rewriteRelativeImportExtensions: true` rewrites these to `.js` on emit
  // so the published artifact still has Node-resolvable extensions.
  if (![".js", ".mjs", ".json", ".ts"].includes(extension)) {
    failures.push(`${relative(file)} imports ${specifier} without a Node ESM extension`);
  }
}

function auditPackageSpecifier(file, specifier) {
  if (BUILTINS.has(specifier)) return;

  const packageName = specifier.startsWith("@")
    ? specifier.split("/").slice(0, 2).join("/")
    : specifier.split("/")[0];

  if (RUNTIME_DEPS.has(packageName)) return;
  if (DEV_DEPS.has(packageName)) {
    failures.push(`${relative(file)} imports devDependency ${packageName} from published runtime code`);
    return;
  }
  failures.push(`${relative(file)} imports undeclared package ${packageName}`);
}

function relative(file) {
  return file.slice(ROOT.length + 1);
}

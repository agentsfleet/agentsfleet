import js from "@eslint/js";
import nPlugin from "eslint-plugin-n";
import promisePlugin from "eslint-plugin-promise";
import importPlugin from "eslint-plugin-import-x";
import globals from "globals";

/*
 * zombiectl is plain ESM JavaScript on Node ≥ 24 + Bun ≥ 1.3.
 * No TypeScript, no React, no JSX — the rule profile mirrors the
 * tier-A / tier-B split used by `ui/packages/{app,website}` but
 * tuned for a CLI runtime: Node best-practices via eslint-plugin-n,
 * async/promise correctness via eslint-plugin-promise, import
 * hygiene via eslint-plugin-import-x.
 *
 * Source under `src/` is the strict tier; `test/` and `scripts/`
 * relax the bars that don't make sense for fixtures + harness code.
 */
/** @type {import("eslint").Linter.Config[]} */
export default [
  {
    ignores: [
      "node_modules/**",
      "coverage/**",
      ".tmp/**",
      "samples/**",
      // Generated/static — owned by skill authors, not lint-target.
      "skills/**",
    ],
  },
  js.configs.recommended,

  // ── Source: src/ + bin/ ──────────────────────────────────────────────
  // Strict tier. Public CLI surface; treat lint findings as bugs.
  {
    files: ["src/**/*.js", "bin/**/*.js"],
    languageOptions: {
      ecmaVersion: 2024,
      sourceType: "module",
      globals: {
        ...globals.node,
        // Bun runtime exposes `Bun.*` + the bun:test imports under test;
        // src/ never imports them directly, but the global keeps a future
        // `Bun.spawn` use lint-clean without disabling globally.
        Bun: "readonly",
      },
    },
    plugins: {
      n: nPlugin,
      promise: promisePlugin,
      "import-x": importPlugin,
    },
    rules: {
      // ── Tier A: built-in correctness ─────────────────────────────────
      "no-var": "error",
      "prefer-const": "error",
      "prefer-template": "error",
      "object-shorthand": ["error", "always"],
      "no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
        },
      ],
      // `value == null` checks null+undefined in one go; strict on that
      // idiom would change semantics across the existing codebase.
      eqeqeq: ["error", "always", { null: "ignore" }],
      "no-throw-literal": "error",
      "no-implicit-coercion": ["error", { allow: ["!!"] }],
      "no-return-assign": ["error", "always"],
      "no-param-reassign": ["error", { props: false }],
      "no-shadow": ["error", { hoist: "functions" }],
      "no-unneeded-ternary": "error",
      "no-useless-rename": "error",
      "no-useless-concat": "error",
      "no-useless-return": "error",
      "no-nested-ternary": "warn",
      "no-multi-assign": "error",
      "no-lonely-if": "error",
      "no-else-return": ["error", { allowElseIf: false }],
      // CLI legitimately calls process.exit; warn for review, don't block.
      "no-process-exit": "warn",
      "no-console": ["warn", { allow: ["warn", "error"] }],

      // ── Tier B: async / promise hygiene ──────────────────────────────
      ...promisePlugin.configs["flat/recommended"].rules,
      "promise/prefer-await-to-then": "error",
      "promise/prefer-await-to-callbacks": "warn",
      "promise/no-return-wrap": "error",
      "promise/no-nesting": "warn",
      "promise/catch-or-return": ["error", { allowFinally: true }],
      "no-async-promise-executor": "error",
      "require-atomic-updates": "error",
      // Floating-promise detection without TypeScript — eslint core has
      // a partial guard; eslint-plugin-promise carries the rest.
      "promise/always-return": ["error", { ignoreLastCallback: true }],

      // ── Tier C: Node best practices ──────────────────────────────────
      // eslint-plugin-n catches deprecated Node APIs, missing files in
      // imports, sync-fs in async paths, and unsupported-by-engines use.
      ...nPlugin.configs["flat/recommended-module"].rules,
      // The CLI is published — `dependencies` vs `devDependencies` matters.
      "n/no-extraneous-import": "error",
      "n/no-missing-import": "error",
      "n/no-process-exit": "off", // covered by core rule above
      "n/no-deprecated-api": "error",
      "n/no-unpublished-import": "off", // bin/ imports src/ which is fine
      "n/no-unsupported-features/node-builtins": [
        "error",
        { ignores: ["fetch", "URL"] },
      ],
      "n/prefer-global/buffer": ["error", "always"],
      "n/prefer-global/process": ["error", "always"],
      "n/prefer-promises/fs": "error",
      "n/prefer-promises/dns": "error",

      // ── Tier D: import hygiene ───────────────────────────────────────
      "import-x/no-cycle": ["error", { maxDepth: 5 }],
      "import-x/no-self-import": "error",
      "import-x/no-useless-path-segments": "error",
      "import-x/no-duplicates": "error",
      "import-x/first": "error",
      "import-x/newline-after-import": "error",
      "import-x/no-relative-packages": "error",
      // Node ESM requires the `.js` extension; enforce it.
      "import-x/extensions": ["error", "ignorePackages"],
    },
    settings: {
      "import-x/resolver": {
        node: { extensions: [".js", ".mjs", ".json"] },
      },
      n: {
        // Match package.json `engines.node`.
        version: ">=24.0.0",
      },
    },
  },

  // ── Tests: test/ ─────────────────────────────────────────────────────
  // Relaxed tier. Fixtures legitimately do "weird" things — knock-out
  // globals, mock fetch, spawn arbitrary commands.
  {
    files: ["test/**/*.js"],
    languageOptions: {
      ecmaVersion: 2024,
      sourceType: "module",
      globals: {
        ...globals.node,
        // bun:test exposes describe/test/expect/beforeEach/afterEach
        // without import-time markers; the test files DO import them
        // explicitly, so we don't add globals — but if a future helper
        // uses them indirectly, the carve-out is here.
        Bun: "readonly",
      },
    },
    plugins: {
      n: nPlugin,
      promise: promisePlugin,
      "import-x": importPlugin,
    },
    rules: {
      "no-var": "error",
      "prefer-const": "error",
      eqeqeq: ["error", "always", { null: "ignore" }],
      "no-throw-literal": "error",
      // Tests legitimately tweak globals (see streamGet's NO_FETCH test
      // that knocks out globalThis.fetch). Don't fight that.
      "no-global-assign": "off",
      // Tests assert against rendered ANSI escape sequences (\x1b[…m)
      // when verifying palette output — \x1b is a legitimate target
      // for the matcher, not an editor accident.
      "no-control-regex": "off",
      // Some output-format / golden tests use multi-space matching
      // because the formatted output IS column-aligned with spaces.
      "no-regex-spaces": "off",
      // A few helpers swallow expected errors via empty catch
      // (unsubscribe-on-already-disposed style).
      "no-empty": ["error", { allowEmptyCatch: true }],
      // Fixture closures sometimes capture an unused argument purely
      // to match an upstream callback signature (e.g. mock fetch
      // handlers that ignore the URL). Relax the unused-arg bar in
      // tests; src/ stays strict.
      "no-unused-vars": [
        "warn",
        { argsIgnorePattern: ".*", varsIgnorePattern: "^_" },
      ],
      // bun:test test() returns a Promise<void> the runner awaits; we
      // don't need promise/always-return enforcement here.
      ...promisePlugin.configs["flat/recommended"].rules,
      "promise/always-return": "off",
      "promise/catch-or-return": "off",
      "import-x/no-duplicates": "error",
      // Some test files load the unit-under-test AFTER setting up
      // module mocks — that pattern legitimately interleaves imports
      // and setup. Source code stays strict via the src/ profile.
      "import-x/first": "off",
      "import-x/extensions": ["error", "ignorePackages"],
      // A few test files use console for diagnostic output; don't gate.
      "no-console": "off",
    },
    settings: {
      "import-x/resolver": { node: { extensions: [".js", ".mjs", ".json"] } },
    },
  },

  // ── Scripts: scripts/ ────────────────────────────────────────────────
  // Build/release glue. Same JS profile as src/ but allows console.
  {
    files: ["scripts/**/*.{js,mjs}"],
    languageOptions: {
      ecmaVersion: 2024,
      sourceType: "module",
      globals: { ...globals.node },
    },
    plugins: { n: nPlugin, promise: promisePlugin },
    rules: {
      "no-var": "error",
      "prefer-const": "error",
      eqeqeq: ["error", "always", { null: "ignore" }],
      "no-console": "off", // scripts legitimately log to stdout
      ...promisePlugin.configs["flat/recommended"].rules,
      "promise/always-return": "off",
      "promise/catch-or-return": "off",
    },
  },
];

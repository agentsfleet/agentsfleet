import js from "@eslint/js";
import tsPlugin from "@typescript-eslint/eslint-plugin";
import tsParser from "@typescript-eslint/parser";
import reactPlugin from "eslint-plugin-react";
import reactHooksPlugin from "eslint-plugin-react-hooks";
import nextPlugin from "@next/eslint-plugin-next";
import jsxA11yPlugin from "eslint-plugin-jsx-a11y";
import globals from "globals";

/** @type {import("eslint").Linter.Config[]} */
export default [
  { ignores: [".next/**", "node_modules/**", "coverage/**", "playwright-report/**"] },
  js.configs.recommended,
  { ...reactPlugin.configs.flat.recommended, settings: { react: { version: "19" } } },
  { ...reactPlugin.configs.flat["jsx-runtime"], settings: { react: { version: "19" } } },
  {
    files: ["app/**/*.{ts,tsx}", "components/**/*.{ts,tsx}", "lib/**/*.{ts,tsx}"],
    ignores: ["**/*.test.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: "module",
        ecmaFeatures: { jsx: true },
        project: "./tsconfig.json",
        tsconfigRootDir: import.meta.dirname,
      },
      globals: { ...globals.browser },
    },
    plugins: {
      "@typescript-eslint": tsPlugin,
      "react-hooks": reactHooksPlugin,
      "@next/next": nextPlugin,
      "jsx-a11y": jsxA11yPlugin,
    },
    settings: { react: { version: "19" } },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      ...reactHooksPlugin.configs.recommended.rules,
      ...nextPlugin.configs.recommended.rules,
      ...nextPlugin.configs["core-web-vitals"].rules,
      ...jsxA11yPlugin.configs.recommended.rules,
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/consistent-type-imports": ["error", { prefer: "type-imports", fixStyle: "separate-type-imports" }],
      "@typescript-eslint/no-non-null-assertion": "error",
      "@typescript-eslint/prefer-as-const": "error",
      // Tier B — typed lint. Catches floating awaits, async-fn in sync slots,
      // and incidental `await` on non-thenables. Worth the parserOptions.project cost.
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
      "@typescript-eslint/await-thenable": "error",
      "@typescript-eslint/prefer-nullish-coalescing": "error",
      "@typescript-eslint/prefer-optional-chain": "error",
      "@typescript-eslint/no-unnecessary-type-assertion": "error",
      "@typescript-eslint/switch-exhaustiveness-check": "error",
      "@typescript-eslint/restrict-template-expressions": ["error", { allowNumber: true, allowBoolean: true, allowNullish: true }],
      "react/no-unstable-nested-components": "error",
      "react/jsx-no-leaked-render": ["error", { validStrategies: ["ternary"] }],
      "no-console": ["warn", { allow: ["warn", "error"] }],
      // `value == null` / `!= null` checks null+undefined in one go;
      // strict-equality on that idiom would change semantics.
      "eqeqeq": ["error", "always", { "null": "ignore" }],
      "no-var": "error",
      "prefer-const": "error",
      "react/prop-types": "off",
      "no-undef": "off",
    },
  },
  {
    files: ["tests/**/*.ts", "playwright.config.ts", "**/*.test.ts", "**/*.test.tsx"],
    languageOptions: {
      parser: tsParser,
      globals: { ...globals.node },
    },
    plugins: { "@typescript-eslint": tsPlugin },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      "no-undef": "off",
      // Tests routinely use `value!` to assert known fixture shape without
      // type-narrowing ceremony. Source code stays strict.
      "@typescript-eslint/no-non-null-assertion": "off",
    },
  },
];

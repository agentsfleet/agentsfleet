import js from "@eslint/js";
import tsPlugin from "@typescript-eslint/eslint-plugin";
import tsParser from "@typescript-eslint/parser";
import reactPlugin from "eslint-plugin-react";
import reactHooksPlugin from "eslint-plugin-react-hooks";
import globals from "globals";

/** @type {import("eslint").Linter.Config[]} */
export default [
  { ignores: ["dist/**", "playwright-report/**", "playwright-results/**", "coverage/**"] },
  js.configs.recommended,
  // React flat config (eslint-plugin-react >=7.35)
  { ...reactPlugin.configs.flat.recommended, settings: { react: { version: "19" } } },
  { ...reactPlugin.configs.flat["jsx-runtime"], settings: { react: { version: "19" } } },
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: "module",
        ecmaFeatures: { jsx: true },
      },
      globals: { ...globals.browser },
    },
    plugins: {
      "@typescript-eslint": tsPlugin,
      "react-hooks": reactHooksPlugin,
    },
    settings: {
      react: { version: "19" },
    },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      ...reactHooksPlugin.configs.recommended.rules,
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
      // TypeScript handles these
      "no-undef": "off",
    },
  },
  {
    files: ["tests/e2e/**/*.ts", "playwright.config.ts"],
    languageOptions: {
      parser: tsParser,
      globals: { ...globals.node },
    },
    plugins: { "@typescript-eslint": tsPlugin },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      "no-undef": "off",
    },
  },
];

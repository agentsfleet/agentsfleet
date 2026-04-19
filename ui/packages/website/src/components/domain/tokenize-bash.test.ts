import { describe, it, expect } from "vitest";
import { tokenizeBash, type Token, type TokenType } from "./tokenize-bash";

function typesOf(tokens: Token[]): TokenType[] {
  return tokens.map((t) => t.type);
}

describe("tokenizeBash — canonical zombiectl demo (§5.8.4 dim 5.8.8)", () => {
  it("classifies the command / flag / default / comment pattern", () => {
    const line = "zombiectl zombie install --template lead-collector # deploys";
    const tokens = tokenizeBash(line);

    // Assert ordered types — whitespace tokens collapse to `default`.
    expect(typesOf(tokens)).toEqual([
      "command",
      "default",
      "default",
      "default",
      "default",
      "default",
      "flag",
      "default",
      "default",
      "default",
      "comment",
    ]);

    // Reassemble → original.
    expect(tokens.map((t) => t.text).join("")).toBe(line);
  });
});

describe("tokenizeBash — table-driven coverage (§5.8.4 dim 5.8.9)", () => {
  const cases: Array<{ label: string; input: string; expect: TokenType }> = [
    { label: "long flag", input: "--template", expect: "flag" },
    { label: "short flag", input: "-v", expect: "flag" },
    { label: "double-quoted string", input: '"hello world"', expect: "string" },
    { label: "single-quoted string", input: "'hello'", expect: "string" },
    { label: "integer", input: "42", expect: "number" },
    { label: "float", input: "3.14", expect: "number" },
    { label: "negative number", input: "-7", expect: "number" },
    { label: "pipe operator", input: "|", expect: "operator" },
    { label: "redirect operator", input: ">", expect: "operator" },
    { label: "absolute path", input: "/etc/hosts", expect: "path" },
    { label: "relative path", input: "./bin/zombiectl", expect: "path" },
    { label: "home path", input: "~/.zombiectl", expect: "path" },
    { label: "https URL", input: "https://usezombie.sh", expect: "path" },
    { label: "variable", input: "$HOME", expect: "variable" },
    { label: "comment", input: "# comment", expect: "comment" },
  ];

  for (const { label, input, expect: expected } of cases) {
    it(`classifies ${label}: \`${input}\` → ${expected}`, () => {
      const tokens = tokenizeBash(input);
      const significant = tokens.filter((t) => t.type !== "default" || t.text.trim().length > 0);
      expect(significant.length).toBeGreaterThan(0);
      expect(significant[0]?.type).toBe(expected);
    });
  }
});

describe("tokenizeBash — preservation contract", () => {
  const corpus = [
    "zombiectl login",
    "curl -sSL https://usezombie.sh/install | bash",
    "zombiectl zombie install --template lead-collector",
    'echo "hello world" > /tmp/out',
    "# starts with comment",
    "",
  ];

  for (const line of corpus) {
    it(`round-trips \`${line}\` without dropping characters`, () => {
      const tokens = tokenizeBash(line);
      expect(tokens.map((t) => t.text).join("")).toBe(line);
    });
  }

  it("classifies the curl install one-liner with a pipe", () => {
    const line = "curl -sSL https://usezombie.sh/install | bash";
    const types = typesOf(tokenizeBash(line));
    expect(types).toContain("command");
    expect(types).toContain("flag");
    expect(types).toContain("path");
    expect(types).toContain("operator");
  });
});

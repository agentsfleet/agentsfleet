import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import ts from "typescript-jsapi";
import { describe, expect, it } from "vitest";

const INTERFACE_COMPONENTS = new Set([
  "Button", "NavItem", "PageTitle", "CardTitle", "SectionLabel",
  "SectionHeader", "DialogTitle", "Badge", "Pagination", "SelectTrigger",
]);
const DESIGN_IMPORTS = new Set([
  "@agentsfleet/design-system", "@agentsfleet/design-system/design-system",
]);
const NON_INTERFACE_FONT = /\bfont-(mono|display|serif)\b|\bfont-\[/;
const APP_ROOT = path.resolve(import.meta.dirname, "..");

function fontOverrides(source: string): string[] {
  const file = ts.createSourceFile("surface.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const names = new Map<string, string>();
  const constants = new Map<string, ts.Expression>();
  const violations: string[] = [];
  function index(node: ts.Node) {
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier) && DESIGN_IMPORTS.has(node.moduleSpecifier.text)) {
      const bindings = node.importClause?.namedBindings;
      if (bindings && ts.isNamedImports(bindings)) {
        for (const item of bindings.elements) names.set(item.name.text, (item.propertyName ?? item.name).text);
      }
    }
    if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.initializer) {
      constants.set(node.name.text, node.initializer);
    }
    ts.forEachChild(node, index);
  }
  index(file);
  function visit(node: ts.Node) {
    if (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) {
      const component = names.get(node.tagName.getText(file));
      if (component && INTERFACE_COMPONENTS.has(component)) {
        for (const attr of node.attributes.properties) {
          if (ts.isJsxAttribute(attr) && attr.name.getText(file) === "className" && attr.initializer) {
            if (containsFontOverride(attr.initializer, constants)) violations.push(component);
          }
        }
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(file);
  return violations;
}

function containsFontOverride(node: ts.Node, constants: Map<string, ts.Expression>, seen = new Set<string>()): boolean {
  if (ts.isStringLiteralLike(node) || ts.isTemplateHead(node) || ts.isTemplateMiddle(node) || ts.isTemplateTail(node)) {
    return NON_INTERFACE_FONT.test(node.text);
  }
  if (ts.isIdentifier(node) && !seen.has(node.text)) {
    const value = constants.get(node.text);
    if (value) return containsFontOverride(value, constants, new Set([...seen, node.text]));
  }
  return ts.forEachChild(node, (child) => containsFontOverride(child, constants, seen) || undefined) ?? false;
}

describe("interface typography ownership", () => {
  it("rejects aliased primitives with multiline, conditional, and named font overrides", () => {
    const source = `import { Button as Action, NavItem } from "@agentsfleet/design-system";
      const LOCAL_STYLE = "font-mono";
      const controls = <><Action className={cn("p-md", active && LOCAL_STYLE)}>Run</Action>
        <NavItem\n className="font-display">Fleets</NavItem></>;`;
    expect(fontOverrides(source)).toEqual(["Button", "NavItem"]);
  });

  it("permits technical fields, technical child values, and layout composition", () => {
    const source = `import { Button, Input } from "@agentsfleet/design-system";
      const controls = <><Input className="font-mono" /><Button className="w-full">
        Copy <code className="font-mono">fleet_id</code></Button></>;`;
    expect(fontOverrides(source)).toEqual([]);
  });

  it("keeps every app page and component on interface primitive defaults", () => {
    const violations: string[] = [];
    for (const directory of ["app", "components"]) {
      const root = path.join(APP_ROOT, directory);
      for (const name of readdirSync(root, { recursive: true, encoding: "utf8" })) {
        if (!/\.[jt]sx$/.test(name) || /\.(test|spec)\./.test(name)) continue;
        const found = fontOverrides(readFileSync(path.join(root, name), "utf8"));
        if (found.length) violations.push(`${directory}/${name}: ${found.join(", ")}`);
      }
    }
    expect(violations).toEqual([]);
  });
});

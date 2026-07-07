import React from "react";

// Shared mock harness for the Models client-component tests (hero, switch
// list, forms, panels). Each shard declares its own hoisted `vi.mock(...)` and
// delegates the factory body here via
// `vi.mock("mod", async () => (await import("./helpers/models-component-mocks")).designSystemStub())`.
// The dynamic import resolves to the same module instance as the shard's static
// import, so the stub a test renders is the one the component sees.

// ── Design-system primitive stubs ───────────────────────────────────────────
// Lightweight DOM-only renders of just the primitives these surfaces use — no
// Radix runtime, so happy-dom stays deterministic. Each preserves the props the
// component / a test assertion reads (testids, ids, aria-*, disabled, onClick).
export function designSystemStub() {
  return {
    // Shared eyebrow typography constant (plain string; components compose it
    // via cn). The stub only needs it to be a defined export.
    EYEBROW_CLASS: "font-mono text-eyebrow uppercase leading-eyebrow tracking-eyebrow",
    Button: ({
      children,
      onClick,
      disabled,
      asChild,
      type,
      // `variant`/`size` are design-system-only props — drop them so React
      // doesn't warn about unknown DOM attributes on the bare button element.
      variant: _variant,
      size: _size,
      ...rest
    }: React.PropsWithChildren<{
      onClick?: () => void;
      disabled?: boolean;
      asChild?: boolean;
      type?: "button" | "submit" | "reset";
      variant?: string;
      size?: string;
    }> &
      Record<string, unknown>) => {
      if (asChild && React.isValidElement(children)) {
        return React.cloneElement(children, {
          "data-size": _size,
          "data-variant": _variant,
        } as Partial<React.HTMLAttributes<HTMLElement>>);
      }
      if (asChild) return children as React.ReactElement;
      // Reflect the disabled state via aria-* rather than the native `disabled`
      // attribute: React suppresses click handlers on truly-disabled buttons, so
      // keeping clicks live lets tests exercise the components' own in-handler
      // re-entrancy guards (which sit behind the same disabled condition).
      return React.createElement(
        "button",
        {
          onClick,
          "aria-disabled": disabled ? "true" : undefined,
          "aria-busy": disabled ? "true" : undefined,
          type: type ?? "button",
          ...rest,
        },
        children,
      );
    },
    Input: (props: React.InputHTMLAttributes<HTMLInputElement>) => React.createElement("input", props),
    Label: ({ children, htmlFor }: React.PropsWithChildren<{ htmlFor?: string }>) =>
      React.createElement("label", { htmlFor }, children),
    Spinner: ({ srLabel }: { srLabel?: string }) =>
      React.createElement("span", { "data-spinner": "1" }, srLabel),
    Alert: ({
      children,
      variant: _variant,
      ...rest
    }: React.PropsWithChildren<{ variant?: string }> & Record<string, unknown>) =>
      React.createElement("div", { role: "alert", ...rest }, children),
    // Select family — native-select stub so onValueChange fires from a change.
    Select: ({
      children,
      value,
      onValueChange,
    }: React.PropsWithChildren<{ value?: string; onValueChange?: (v: string) => void }>) =>
      React.createElement(
        "div",
        { "data-select": "1", "data-value": value },
        React.createElement(
          "select",
          {
            "data-select-native": "1",
            value,
            onChange: (e: React.ChangeEvent<HTMLSelectElement>) => onValueChange?.(e.target.value),
          },
          children,
        ),
      ),
    SelectTrigger: ({ children, id, ...rest }: React.PropsWithChildren<{ id?: string }> & Record<string, unknown>) =>
      React.createElement("span", { "data-select-trigger": "1", id, ...rest }, children),
    SelectValue: ({ placeholder }: { placeholder?: string }) =>
      React.createElement("span", { "data-select-placeholder": "1" }, placeholder),
    SelectContent: ({ children }: React.PropsWithChildren) =>
      React.createElement("span", { "data-select-content": "1" }, children),
    SelectItem: ({ children, value }: React.PropsWithChildren<{ value: string }>) =>
      React.createElement("option", { value }, children),
    DashboardPanel: ({ children, ...rest }: React.PropsWithChildren<Record<string, unknown>>) =>
      React.createElement("div", { ...rest }, children),
    MetaGrid: ({ items }: { items: { label: string; value: React.ReactNode }[] }) =>
      React.createElement(
        "dl",
        { "data-meta-grid": "1" },
        ...items.map((i) =>
          React.createElement(
            "div",
            { key: i.label },
            React.createElement("dt", null, i.label),
            React.createElement("dd", null, i.value),
          ),
        ),
      ),
    StatusPill: ({ children, variant }: React.PropsWithChildren<{ variant?: string }>) =>
      React.createElement("span", { "data-status-pill": variant ?? "default" }, children),
    SectionLabel: ({ children }: React.PropsWithChildren) =>
      React.createElement("h2", { "data-section-label": "1" }, children),
    DashboardRowGroup: ({ children, ...rest }: React.PropsWithChildren<Record<string, unknown>>) =>
      React.createElement("div", { ...rest }, children),
    DashboardRow: ({
      icon,
      title,
      description,
      meta,
      action,
      ...rest
    }: {
      icon?: React.ReactNode;
      title: React.ReactNode;
      description?: React.ReactNode;
      meta?: React.ReactNode;
      action?: React.ReactNode;
    } & Record<string, unknown>) =>
      React.createElement(
        "div",
        { "data-row": typeof title === "string" ? title : "row", ...rest },
        icon,
        React.createElement("span", { "data-row-title": "1" }, title),
        React.createElement("span", { "data-row-desc": "1" }, description),
        React.createElement("span", { "data-row-action": "1" }, action),
        meta ? React.createElement("span", { "data-row-meta": "1" }, meta) : null,
      ),
  };
}

// Lucide icon stubs used across the switch list / hero.
const ICONS = ["CpuIcon", "LinkIcon", "ServerIcon", "LockIcon", "Trash2Icon"] as const;
export function lucideStub() {
  const make = (name: string) => {
    const C = (p: Record<string, unknown>) => React.createElement("svg", { ...p, "data-icon": name });
    C.displayName = name;
    return C;
  };
  return Object.fromEntries(ICONS.map((n) => [n, make(n)]));
}

// A controllable `ProviderModelSelect` stub: renders a labelled input wired to
// onModelChange so form tests can set a model without the real catalogue picker.
export function providerModelSelectStub() {
  return {
    default: ({ id, model, onModelChange, label = "Model" }: { id: string; model: string; onModelChange: (v: string) => void; label?: string }) =>
      React.createElement(
        "div",
        null,
        React.createElement("label", { htmlFor: id }, label),
        React.createElement("input", {
          id,
          "aria-label": label,
          value: model,
          onChange: (e: React.ChangeEvent<HTMLInputElement>) => onModelChange(e.target.value),
        }),
      ),
  };
}

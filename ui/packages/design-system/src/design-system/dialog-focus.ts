const FIELD = 'input:enabled:not([readonly]):not([type="hidden"]):not([type="checkbox"]):not([type="radio"]), textarea:enabled:not([readonly]), select:enabled, [role="combobox"]:enabled';

function firstField(container: HTMLElement): HTMLElement | undefined {
  return [...container.querySelectorAll<HTMLElement>(FIELD)].find(field => !field.closest('[hidden], [aria-hidden="true"]'));
}

/** Radix retains trapping and restoration; a late form gets one initial focus opportunity. */
export function focusDialogField(container: HTMLElement, event: Event): (() => void) | null {
  const field = firstField(container);
  if (field) {
    field.focus({preventScroll: true});
    event.preventDefault();
    return null;
  }
  const cleanup = () => {
    observer.disconnect();
    container.ownerDocument.removeEventListener("keydown", cleanup, true);
    container.ownerDocument.removeEventListener("pointerdown", cleanup, true);
  };
  const observer = new MutationObserver(() => {
    if (!container.isConnected || container.dataset.state === "closed") {cleanup(); return;}
    const target = firstField(container);
    if (!target) return;
    const active = container.ownerDocument.activeElement;
    const available = active === container || active === container.ownerDocument.body
      || active === container.querySelector("[data-dialog-close]");
    cleanup();
    if (available) target.focus({preventScroll: true});
  });
  observer.observe(container, {childList: true, subtree: true, attributes: true, attributeFilter: ["disabled", "readonly", "hidden", "aria-hidden", "data-state"]});
  container.ownerDocument.addEventListener("keydown", cleanup, true);
  container.ownerDocument.addEventListener("pointerdown", cleanup, true);
  return cleanup;
}

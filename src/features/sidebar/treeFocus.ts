import { useEffect, type RefObject } from "react";
import type { TreeItem } from "./treeKeys";

const ROW = '[role="treeitem"]';

/** Every shown row of the tree that holds `row`, in document order, which is the order the user sees. */
export function treeRows(row: HTMLElement): HTMLElement[] {
  const tree = row.closest('[role="tree"]');
  return tree ? [...tree.querySelectorAll<HTMLElement>(ROW)] : [row];
}

export const treeItem = (row: HTMLElement): TreeItem => ({
  level: Number(row.getAttribute("aria-level")),
  expandable: row.hasAttribute("aria-expanded"),
  expanded: row.getAttribute("aria-expanded") === "true",
});

export function focusRow(row: HTMLElement | null | undefined): void {
  if (!row) return;
  row.focus({ preventScroll: true });
  row.scrollIntoView({ block: "nearest" });
}

export function focusFirstRow(tree: HTMLElement | null): void {
  focusRow(tree?.querySelector<HTMLElement>(ROW));
}

/**
 * Keeps exactly one row of `tree` at tabindex 0: the last one focused, or the first row once that one is gone.
 * React renders every row at -1 and never writes that prop again, so this hook owns the one exception.
 */
export function useRovingTabStop(tree: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const root = tree.current;
    if (!root) return;
    let stop: HTMLElement | null = null;
    const promote = (row: HTMLElement) => {
      if (stop && stop !== row) stop.tabIndex = -1;
      row.tabIndex = 0;
      stop = row;
    };
    const settle = () => {
      if (stop?.isConnected) return;
      stop = null;
      const first = root.querySelector<HTMLElement>(ROW);
      if (first) promote(first);
    };
    const onFocusIn = (e: FocusEvent) => {
      if (e.target instanceof HTMLElement && e.target.matches(ROW)) promote(e.target);
    };
    const rows = new MutationObserver(settle);
    rows.observe(root, { childList: true, subtree: true });
    root.addEventListener("focusin", onFocusIn);
    settle();
    return () => {
      rows.disconnect();
      root.removeEventListener("focusin", onFocusIn);
    };
  }, [tree]);
}

/** Sent to the tree's element to move focus to its first row, which may be scrolled out of the rendered window. */
export const FOCUS_FIRST_ROW = "l1-tree-focus-first";

export function focusRow(row: HTMLElement | null | undefined): void {
  if (!row) return;
  row.focus({ preventScroll: true });
  row.scrollIntoView({ block: "nearest" });
}

export function focusFirstRow(tree: HTMLElement | null): void {
  tree?.dispatchEvent(new Event(FOCUS_FIRST_ROW));
}

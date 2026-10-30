/** A shown tree row as the keyboard sees it. `level` is 1-based, as `aria-level` is. */
export type TreeItem = { level: number; expandable: boolean; expanded: boolean };

export type TreeAction = { kind: "focus"; index: number } | { kind: "expand" } | { kind: "collapse" } | { kind: "activate" };

type Rule = (items: readonly TreeItem[], at: number, item: TreeItem) => TreeAction | null;

const focus = (index: number): TreeAction => ({ kind: "focus", index });

function parentOf(items: readonly TreeItem[], at: number, level: number): number | null {
  for (let i = at - 1; i >= 0; i--) if ((items[i]?.level ?? level) < level) return i;
  return null;
}

/** The WAI-ARIA tree pattern's keys over the shown rows in document order. */
const RULES = new Map<string, Rule>([
  ["ArrowDown", (items, at) => (at + 1 < items.length ? focus(at + 1) : null)],
  ["ArrowUp", (_, at) => (at > 0 ? focus(at - 1) : null)],
  [
    "ArrowRight",
    (items, at, item) => {
      if (!item.expandable) return null;
      if (!item.expanded) return { kind: "expand" };
      const child = items[at + 1];
      return child && child.level > item.level ? focus(at + 1) : null;
    },
  ],
  [
    "ArrowLeft",
    (items, at, item) => {
      if (item.expandable && item.expanded) return { kind: "collapse" };
      const parent = parentOf(items, at, item.level);
      return parent === null ? null : focus(parent);
    },
  ],
  ["Home", () => focus(0)],
  ["End", (items) => focus(items.length - 1)],
  ["Enter", () => ({ kind: "activate" })],
]);

/** What `key` does on the row at `at`, or null when the tree ignores it there. */
export function treeKey(key: string, items: readonly TreeItem[], at: number): TreeAction | null {
  const item = items[at];
  if (!item) return null;
  return RULES.get(key)?.(items, at, item) ?? null;
}

/**
 * The row type-ahead lands on: the next one, wrapping, whose name starts with `typed`. One letter, or the same letter
 * typed again, starts past the current row and so cycles through the rows it opens; longer text keeps the current row
 * while it still matches.
 */
export function typeAhead(names: readonly string[], at: number, typed: string): number | null {
  const lower = typed.toLocaleLowerCase();
  const repeated = [...lower].every((c) => c === lower[0]);
  const prefix = repeated ? lower.slice(0, 1) : lower;
  const from = repeated ? at + 1 : at;
  for (let step = 0; step < names.length; step++) {
    const i = (from + step) % names.length;
    if (names[i]?.toLocaleLowerCase().startsWith(prefix)) return i;
  }
  return null;
}

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

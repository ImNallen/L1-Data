import { expect, it } from "vitest";
import { treeKey, type TreeItem } from "./treeKeys";

const open = (level: number): TreeItem => ({ level, expandable: true, expanded: true });
const closed = (level: number): TreeItem => ({ level, expandable: true, expanded: false });
const leaf = (level: number): TreeItem => ({ level, expandable: false, expanded: false });

// prod / shop / public / orders (closed), customers (open) with two columns, then a second connection.
const tree = [open(1), open(2), open(3), closed(4), open(4), leaf(5), leaf(5), closed(1)];

it("moves one row down and up, and stops at either end", () => {
  expect(treeKey("ArrowDown", tree, 0)).toEqual({ kind: "focus", index: 1 });
  expect(treeKey("ArrowUp", tree, 5)).toEqual({ kind: "focus", index: 4 });
  expect(treeKey("ArrowDown", tree, 7)).toBeNull();
  expect(treeKey("ArrowUp", tree, 0)).toBeNull();
});

it("opens a closed row with Right, then moves into its first child", () => {
  expect(treeKey("ArrowRight", tree, 3)).toEqual({ kind: "expand" });
  expect(treeKey("ArrowRight", tree, 4)).toEqual({ kind: "focus", index: 5 });
});

it("does nothing on Right at a leaf, or at an open row with no children shown", () => {
  expect(treeKey("ArrowRight", tree, 5)).toBeNull();
  expect(treeKey("ArrowRight", [open(1), open(1)], 0)).toBeNull();
});

it("closes an open row with Left, and from a leaf or a closed row moves to the parent", () => {
  expect(treeKey("ArrowLeft", tree, 4)).toEqual({ kind: "collapse" });
  expect(treeKey("ArrowLeft", tree, 6)).toEqual({ kind: "focus", index: 4 });
  expect(treeKey("ArrowLeft", tree, 3)).toEqual({ kind: "focus", index: 2 });
  expect(treeKey("ArrowLeft", tree, 7)).toBeNull();
});

it("jumps to the first and last shown rows with Home and End", () => {
  expect(treeKey("Home", tree, 5)).toEqual({ kind: "focus", index: 0 });
  expect(treeKey("End", tree, 1)).toEqual({ kind: "focus", index: 7 });
});

it("activates the row with Enter and ignores other keys", () => {
  expect(treeKey("Enter", tree, 3)).toEqual({ kind: "activate" });
  expect(treeKey("a", tree, 3)).toBeNull();
  expect(treeKey("ArrowDown", tree, 8)).toBeNull();
});

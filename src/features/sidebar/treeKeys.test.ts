import { expect, it } from "vitest";
import { treeKey, typeAhead, type TreeItem } from "./treeKeys";

const open = (level: number): TreeItem => ({ level, expandable: true, expanded: true });
const closed = (level: number): TreeItem => ({ level, expandable: true, expanded: false });
const leaf = (level: number): TreeItem => ({ level, expandable: false, expanded: false });

const rows = { prod: open(1), shop: open(2), schema: open(3), orders: closed(4), customers: open(4), id: leaf(5), total: leaf(5), staging: closed(1) };
const tree = Object.values(rows);
const at = Object.fromEntries(Object.keys(rows).map((name, i) => [name, i])) as Record<keyof typeof rows, number>;

it("moves one row down and up, and stops at either end", () => {
  expect(treeKey("ArrowDown", tree, at.prod)).toEqual({ kind: "focus", index: at.shop });
  expect(treeKey("ArrowUp", tree, at.id)).toEqual({ kind: "focus", index: at.customers });
  expect(treeKey("ArrowDown", tree, at.staging)).toBeNull();
  expect(treeKey("ArrowUp", tree, at.prod)).toBeNull();
});

it("opens a closed row with Right, then moves into its first child", () => {
  expect(treeKey("ArrowRight", tree, at.orders)).toEqual({ kind: "expand" });
  expect(treeKey("ArrowRight", tree, at.customers)).toEqual({ kind: "focus", index: at.id });
});

it("does nothing on Right at a leaf, or at an open row with no children shown", () => {
  expect(treeKey("ArrowRight", tree, at.id)).toBeNull();
  expect(treeKey("ArrowRight", [open(1), open(1)], 0)).toBeNull();
});

it("closes an open row with Left, and from a leaf or a closed row moves to the parent", () => {
  expect(treeKey("ArrowLeft", tree, at.customers)).toEqual({ kind: "collapse" });
  expect(treeKey("ArrowLeft", tree, at.total)).toEqual({ kind: "focus", index: at.customers });
  expect(treeKey("ArrowLeft", tree, at.orders)).toEqual({ kind: "focus", index: at.schema });
  expect(treeKey("ArrowLeft", tree, at.staging)).toBeNull();
});

it("jumps to the first and last shown rows with Home and End", () => {
  expect(treeKey("Home", tree, at.id)).toEqual({ kind: "focus", index: at.prod });
  expect(treeKey("End", tree, at.shop)).toEqual({ kind: "focus", index: at.staging });
});

it("activates the row with Enter and ignores other keys", () => {
  expect(treeKey("Enter", tree, at.orders)).toEqual({ kind: "activate" });
  expect(treeKey("a", tree, at.orders)).toBeNull();
  expect(treeKey("ArrowDown", tree, tree.length)).toBeNull();
});

it("types ahead to the next row whose name starts with the letters typed, wrapping past the end", () => {
  const names = ["prod", "shop", "public", "orders", "order_items", "payments"];
  expect(typeAhead(names, 0, "o")).toBe(3);
  expect(typeAhead(names, 3, "o")).toBe(4);
  expect(typeAhead(names, 4, "o")).toBe(3);
  expect(typeAhead(names, 3, "order_")).toBe(4);
  expect(typeAhead(names, 3, "orders")).toBe(3);
  expect(typeAhead(names, 5, "P")).toBe(0);
  expect(typeAhead(names, 3, "oo")).toBe(4);
  expect(typeAhead(names, 0, "z")).toBeNull();
});

import type { ReactElement } from "react";
import { afterEach, expect, it } from "vitest";
import type { FunctionInfo } from "../../ipc/bindings";
import { useWorkspace } from "../../state/workspace";
import { dropEditor, moveCursor, seedEditor, textOf } from "../editor/editorStates";
import { FunctionRow } from "./ConnectionTree";

afterEach(() => {
  for (const id of ["active", "other"]) dropEditor(id);
  useWorkspace.setState(useWorkspace.getInitialState(), true);
});

function doubleClick(fn: FunctionInfo) {
  const row: ReactElement<{ onDoubleClick: () => void }> = FunctionRow({ path: "connection/shop/schema/#functions", depth: 4, fn });
  row.props.onDoubleClick();
}

it.each([
  { kind: "Function", insertText: "[odd]]schema].[scalar]]name]()", prefix: "SELECT " },
  { kind: "Function", insertText: "[odd]]schema].[table]]name](NULL, NULL)", prefix: "SELECT * FROM " },
  { kind: "Procedure", insertText: "EXEC [odd]]schema].[proc]]name] NULL, NULL", prefix: "" },
  { kind: "Function", insertText: "order_count()", prefix: "SELECT " },
] satisfies { kind: FunctionInfo["kind"]; insertText: string; prefix: string }[])("inserts the catalog's $kind template verbatim at the active cursor", ({ kind, insertText, prefix }) => {
  seedEditor("active", `${prefix}; -- retain suffix`);
  moveCursor("active", prefix.length);
  seedEditor("other", "SELECT 99");
  useWorkspace.setState({ active: "active" });

  doubleClick({ oid: 12, name: "display name", args: "unparsed display arguments", returns: "int", kind, insertText });

  expect(textOf("active")).toBe(`${prefix}${insertText}; -- retain suffix`);
  expect(textOf("other")).toBe("SELECT 99");
});

it("does not insert a routine when no tab is active", () => {
  seedEditor("other", "SELECT 99");
  useWorkspace.setState({ active: null });
  doubleClick({ oid: 12, name: "proc", args: "", returns: "", kind: "Procedure", insertText: "EXEC [dbo].[proc]" });
  expect(textOf("other")).toBe("SELECT 99");
});

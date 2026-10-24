// @vitest-environment jsdom
import { EditorView } from "@codemirror/view";
import { afterEach, expect, it } from "vitest";
import { dropEditor, editorStateOf, seedEditor } from "./editorStates";

afterEach(() => dropEditor("t1"));

it("names the query editor for assistive technology", () => {
  seedEditor("t1", "SELECT 1");
  const view = new EditorView({ state: editorStateOf("t1"), parent: document.body });
  expect(view.contentDOM.getAttribute("role")).toBe("textbox");
  expect(view.contentDOM.getAttribute("aria-label")).toBe("SQL editor");
  view.destroy();
});

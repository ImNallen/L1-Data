import { sql } from "@codemirror/lang-sql";
import { EditorState } from "@codemirror/state";
import { drawSelection, EditorView, lineNumbers } from "@codemirror/view";
import { useEffect, useRef } from "react";
import type { DatabaseKind } from "../../ipc/bindings";
import { SQL_DIALECT } from "../editor/sqlDialect";
import { editorHighlighting, editorTheme, functionCallStyle } from "../editor/theme";

export function HistorySql({ text, databaseKind }: { text: string; databaseKind: DatabaseKind }) {
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!host.current) return;
    const view = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: text,
        extensions: [
          EditorState.readOnly.of(true), EditorView.editable.of(false),
          EditorView.contentAttributes.of({ "aria-label": "Saved SQL", tabindex: "0" }),
          lineNumbers(), drawSelection(), EditorView.lineWrapping,
          sql({ dialect: SQL_DIALECT[databaseKind] }), editorTheme, editorHighlighting, functionCallStyle,
        ],
      }),
    });
    return () => view.destroy();
  }, [text, databaseKind]);
  return <div ref={host} data-testid="history-sql" className="h-full min-h-0" />;
}

import { highlightTree } from "@lezer/highlight";
import { useMemo } from "react";
import type { DatabaseKind } from "../../ipc/bindings";
import { cx } from "../../ui/primitives";
import { SQL_DIALECT } from "../editor/sqlDialect";
import { isKeywordCall, staticSqlHighlighter } from "../editor/theme";

type Span = { text: string; className: string };

export function sqlSpans(sql: string, databaseKind: DatabaseKind = "Postgres"): Span[] {
  const tree = SQL_DIALECT[databaseKind].language.parser.parse(sql);
  const calls = new Set<number>();
  tree.iterate({
    enter(node) {
      if (node.name === "Keyword" && isKeywordCall(sql.slice(node.from, node.to), sql.slice(node.to, node.to + 1))) calls.add(node.from);
    },
  });
  const spans: Span[] = [];
  let at = 0;
  highlightTree(tree, staticSqlHighlighter, (from, to, classes) => {
    if (from > at) spans.push({ text: sql.slice(at, from), className: "" });
    spans.push({ text: sql.slice(from, to), className: calls.has(from) ? "" : classes });
    at = to;
  });
  if (at < sql.length) spans.push({ text: sql.slice(at), className: "" });
  return spans;
}

export function SqlHighlight({ sql, databaseKind, className }: { sql: string; databaseKind: DatabaseKind; className?: string }) {
  const spans = useMemo(() => sqlSpans(sql, databaseKind), [sql, databaseKind]);
  return (
    <pre className={cx("selectable font-mono text-code-sm leading-[1.6] break-words whitespace-pre-wrap text-(color:--code)", className)}>
      {spans.map((s, i) => (s.className ? <span key={i} className={s.className}>{s.text}</span> : s.text))}
    </pre>
  );
}

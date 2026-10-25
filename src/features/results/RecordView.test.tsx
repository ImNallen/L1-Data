// @vitest-environment jsdom
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import type { ResultMeta, RowPage } from "../../ipc/bindings";
import { dropRowCaches } from "../../state/rowCache";
import { RecordView } from "./RecordView";

const meta: ResultMeta = {
  databaseKind: "Postgres",
  ref: { tabId: "r1", runId: 1, index: 0 },
  db: { connectionId: "c1", database: "shop" },
  columns: [
    { name: "id", type: { display: "bigint", category: "Integer" }, source: null },
    { name: "doc", type: { display: "jsonb", category: "Json" }, source: null },
    { name: "note", type: { display: "text", category: "Text" }, source: null },
  ],
  rowCount: 600,
  rowsSeen: 600,
  truncated: null,
  elapsedMs: 1,
  allRows: { kind: "Statement" },
};

const long = "x".repeat(5_000);
const pages: number[] = [];

afterEach(() => {
  cleanup();
  clearMocks();
  dropRowCaches("r1");
  pages.length = 0;
});

function serve() {
  mockIPC((cmd, args) => {
    const a = args as { start: number; len: number; row: number };
    if (cmd === "result_page") {
      pages.push(a.start);
      const rows = Array.from({ length: a.len }, (_, i) => [String(a.start + i + 1), `{"n": ${a.start + i}}`, a.start + i === 0 ? long.slice(0, 2_000) : null]);
      return { start: a.start, rows, cut: a.start === 0 ? [[0, 2]] : [] } satisfies RowPage;
    }
    if (cmd === "result_cell" && a.row === 0) return long;
    throw new Error(`not served: ${cmd}`);
  });
}

it("lists every column with its type glyph and its full value", async () => {
  serve();
  render(<RecordView meta={meta} row={0} onClose={() => {}} />);
  await vi.waitFor(() => expect(screen.getByTestId("record-value-2").textContent).toBe(long));
  expect(screen.getByTestId("record-position").textContent).toBe("Row 1 of 600");
  expect(screen.getByTestId("record-value-1").textContent).toBe('{"n": 0}');
  expect(
    meta.columns.map(
      (_, i) =>
        screen
          .getByTestId(`record-field-${i}`)
          .querySelector("svg")
          ?.getAttribute("class")
          ?.match(/lucide-([a-z-]+)/)?.[1],
    ),
  ).toEqual(["hash", "braces", "case-sensitive"]);
  expect(screen.getByTestId("record-value-0").textContent).toBe("1");
});

it("pages through rows with the arrow keys, loading each page through the row cache", async () => {
  serve();
  const user = userEvent.setup();
  render(<RecordView meta={meta} row={0} onClose={() => {}} />);
  await vi.waitFor(() => expect(screen.getByTestId("record-value-0").textContent).toBe("1"));
  for (let i = 0; i < 300; i++) await user.keyboard("{ArrowDown}");
  await vi.waitFor(() => expect(screen.getByTestId("record-value-0").textContent).toBe("301"));
  expect([screen.getByTestId("record-position").textContent, screen.getByTestId("record-value-2").textContent, pages]).toEqual([
    "Row 301 of 600",
    "NULL",
    [0, 256],
  ]);
  await user.keyboard("{ArrowUp}");
  expect(screen.getByTestId("record-position").textContent).toBe("Row 300 of 600");
});

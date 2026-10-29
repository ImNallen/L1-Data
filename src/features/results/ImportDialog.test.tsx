// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, expect, it } from "vitest";
import type { ImportPreview } from "../../ipc/bindings";
import { ImportMapping } from "./ImportDialog";

afterEach(cleanup);

it("shows engine header suggestions and lets the user reorder or skip fields", async () => {
  const preview: ImportPreview = {
    source: "source",
    target: { db: { connectionId: "connection", database: "shop" }, schema: "public", table: "target", oid: 42 },
    bytes: 24,
    rows: 2,
    fields: ["name", "id"],
    mapping: ["name", "id"],
    sample: [["a", "1"]],
    truncated: false,
    columns: ["id", "name"].map((name, i) => ({
      name,
      attnum: i + 1,
      writable: true,
      quoted: name,
      typeDisplay: "text",
      category: "Text",
      notNull: false,
      pk: false,
      fk: false,
    })),
  };
  function Mapping() {
    const [mapping, setMapping] = useState(preview.mapping);
    return (
      <>
        <ImportMapping preview={preview} mapping={mapping} onChange={setMapping} />
        <output data-testid="mapping">{JSON.stringify(mapping)}</output>
      </>
    );
  }
  render(<Mapping />);
  expect(screen.getByTestId("import-map-0")).toHaveProperty("value", "name");
  expect(screen.getByTestId("import-map-1")).toHaveProperty("value", "id");
  const user = userEvent.setup();
  await user.selectOptions(screen.getByTestId("import-map-0"), "id");
  await user.selectOptions(screen.getByTestId("import-map-1"), "name");
  expect(screen.getByTestId("mapping").textContent).toBe('["id","name"]');
  await user.selectOptions(screen.getByTestId("import-map-1"), "");
  expect(screen.getByTestId("mapping").textContent).toBe('["id",null]');
});

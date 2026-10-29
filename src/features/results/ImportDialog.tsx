import type { ImportPreview } from "../../ipc/bindings";
import { useLive } from "../../state/live";
import { useWorkspace } from "../../state/workspace";
import { Button, Modal, Spinner, inputClass } from "../../ui/primitives";
import { cancelImport, changeHeader, changeMapping, submitImport, useImport } from "./importState";

export function ImportMapping({
  preview,
  mapping,
  onChange,
  disabled = false,
}: {
  preview: ImportPreview;
  mapping: (string | null)[];
  onChange: (mapping: (string | null)[]) => void;
  disabled?: boolean;
}) {
  return (
    <div className="max-h-[24vh] overflow-auto rounded-lg border border-grid">
      <table className="w-full text-left text-ui-12.5">
        <thead className="sticky top-0 bg-raised text-dim">
          <tr>
            <th className="px-3 py-2">CSV field</th>
            <th className="px-3 py-2">Table column</th>
          </tr>
        </thead>
        <tbody>
          {preview.fields.map((name, index) => (
            <tr key={index} className="border-t border-grid">
              <td className="max-w-64 truncate px-3 py-1.5 font-mono" title={name}>
                {name || `Column ${index + 1}`}
              </td>
              <td className="px-3 py-1.5">
                <select
                  aria-label={`Map CSV field ${index + 1}`}
                  data-testid={`import-map-${index}`}
                  className={inputClass}
                  disabled={disabled}
                  value={mapping[index] ?? ""}
                  onChange={(event) => onChange(mapping.map((value, at) => (at === index ? event.target.value || null : value)))}
                >
                  <option value="">Skip field</option>
                  {preview.columns.map((column) => (
                    <option key={column.name} value={column.name}>
                      {column.name} · {column.typeDisplay}
                      {column.notNull ? " · required" : ""}
                    </option>
                  ))}
                </select>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function ImportDialog() {
  const prompt = useImport((state) => state.prompt);
  const working = useLive((state) => (prompt ? state.consoles[prompt.intent.tab.id]?.import?.source === prompt.source : false));
  const asking = useWorkspace((state) => state.overlay.kind === "ask");
  if (!prompt || working || asking) return null;
  const ready = prompt.phase === "ready" || prompt.phase === "submitting" ? prompt : null;
  const selected = ready?.mapping.filter((column) => column !== null) ?? [];
  const duplicate = new Set(selected).size !== selected.length;
  return (
    <Modal
      open
      title={`Import CSV into ${prompt.intent.target.schema}.${prompt.intent.target.table}`}
      description={prompt.intent.file}
      width={640}
      testId="import-dialog"
      onOpenChange={(open) => !open && void cancelImport()}
    >
      {prompt.phase === "preparing" && (
        <div role="status" data-testid="import-preparing" className="mb-5 flex items-center gap-2 text-ui-13 text-dim">
          <Spinner />
          Preparing a snapshot and checking the CSV…
        </div>
      )}
      {prompt.phase === "failed" && (
        <p role="alert" data-testid="import-error" className="mb-4 text-ui-13 whitespace-pre-wrap text-danger">
          {prompt.message}
        </p>
      )}
      {ready && (
        <div className="mb-4 flex flex-col gap-3">
          <div className="flex items-center justify-between gap-3 text-ui-12.5">
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                data-testid="import-header"
                checked={ready.header}
                disabled={ready.phase !== "ready"}
                onChange={(event) => changeHeader(event.target.checked)}
              />
              First record is a header
            </label>
            <span data-testid="import-counts" className="text-dim tabular-nums">
              {ready.preview.rows.toLocaleString()} rows · {ready.preview.bytes.toLocaleString()} bytes
            </span>
          </div>
          <ImportMapping preview={ready.preview} mapping={ready.mapping} onChange={changeMapping} disabled={ready.phase !== "ready"} />
          <p className="text-ui-12 text-dim">Unmapped table columns use their defaults. An unquoted empty field is NULL; a quoted empty field is empty text.</p>
          <div data-testid="import-sample" className="max-h-32 overflow-auto rounded-lg border border-grid">
            <table className="text-left font-mono text-ui-12">
              <tbody>
                {ready.preview.sample.map((row, index) => (
                  <tr key={index} className="border-b border-grid">
                    {row.map((value, field) => (
                      <td key={field} className="max-w-48 truncate border-r border-grid px-2 py-1" title={value ?? "NULL"}>
                        {value === null ? <i className="text-dim">NULL</i> : value === "" ? <i className="text-dim">empty</i> : value}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {ready.preview.truncated && <p className="text-ui-12 text-dim">The sample is shortened. Import uses the complete values.</p>}
          {duplicate && (
            <p role="alert" className="text-ui-12 text-danger">
              Choose each table column only once.
            </p>
          )}
        </div>
      )}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" data-testid="import-cancel" onClick={() => void cancelImport()}>
          {prompt.phase === "failed" ? "Close" : "Cancel"}
        </Button>
        {ready && (
          <Button
            variant="primary"
            data-testid="import-submit"
            disabled={ready.phase !== "ready" || duplicate || selected.length === 0 || ready.preview.rows === 0}
            onClick={() => void submitImport()}
          >
            {ready.phase === "submitting" ? "Starting…" : `Import ${ready.preview.rows.toLocaleString()} rows`}
          </Button>
        )}
      </div>
    </Modal>
  );
}

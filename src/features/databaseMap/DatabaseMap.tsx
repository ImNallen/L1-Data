import { XIcon } from "lucide-react";
import { useEffect } from "react";
import { Button, IconButton, cx } from "../../ui/primitives";
import { closeDatabaseMap, openDatabaseMap, startMapLifecycle, toggleMapOverlay, useDatabaseMap } from "./controller";
import { MapCanvas } from "./MapCanvas";
import { MapDetails } from "./MapDetails";

export function DatabaseMap() {
  const map = useDatabaseMap((s) => s.map);
  useEffect(startMapLifecycle, []);
  if (map.kind === "closed") return null;
  return (
    <section data-testid="database-map" aria-label="Database map" className="flex h-full min-w-0 flex-col bg-bg">
      <header className="flex shrink-0 items-center justify-between gap-2 border-b border-border px-3 py-2">
        <div className="min-w-0">
          <h2 className="text-ui-13 font-medium text-fg">Database map</h2>
          <p className="truncate text-ui-10.5 text-dim" title={map.db.database}>
            {map.db.database} · Declared relationships
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {map.kind === "exploring" && (
            <button
              type="button"
              data-testid="map-overlay"
              aria-pressed={map.analysis.kind !== "off"}
              className={cx(
                "rounded-md border px-2 py-1.5 text-ui-11",
                map.analysis.kind !== "off" ? "border-accent-border bg-badge-bg text-badge" : "border-control text-muted hover:bg-hover",
              )}
              onClick={toggleMapOverlay}
            >
              Active query
            </button>
          )}
          <IconButton data-testid="map-close" aria-label="Close database map" title="Close database map" onClick={closeDatabaseMap}>
            <XIcon size={15} />
          </IconButton>
        </div>
      </header>
      {map.kind === "unavailable" ? (
        <div
          data-testid="map-unavailable"
          role="status"
          className="flex flex-1 flex-col items-center justify-center gap-4 p-6 text-center text-ui-12 text-muted"
        >
          <p>{map.message}</p>
          <Button data-testid="map-reload" onClick={() => openDatabaseMap(map.db, map.root)}>
            Reload map
          </Button>
        </div>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-border px-3 py-2 text-ui-10.5 text-dim">
            <span>← Incoming</span>
            <span>Focus</span>
            <span>Outgoing →</span>
            <span className="ml-auto">
              {map.graph.nodes.size} tables · {map.graph.edges.size} keys
            </span>
          </div>
          <MapCanvas map={map} />
          <div className="flex flex-wrap gap-x-4 gap-y-1 px-3 py-1.5 text-ui-10.5 text-dim">
            <span>━ Declared FK</span>
            <span className="text-keyword">┄ SQL equality</span>
            <span className="text-amber">┄ Name match (tentative)</span>
            <span title="Only metadata visible to this login is shown. Catalog versions are refresh epochs; refresh after external DDL.">
              Visible FK columns only
            </span>
          </div>
          <MapDetails map={map} />
        </>
      )}
    </section>
  );
}

import * as Menu from "@radix-ui/react-dropdown-menu";
import { ChevronDownIcon, ListIndentIncreaseIcon, PlayIcon, SquareIcon } from "lucide-react";
import { ConnectionColorDot } from "../features/connections/connectionColors";
import { cancelActive, endTransaction, formatActive, runOrChoose, setTabMode, setTabTarget } from "../features/editor/runActions";
import type { ConnectionState, DatabaseRef, Environment, TransactionLoss, TxnMode } from "../ipc/bindings";
import { homeDb } from "../state/dbKey";
import { instanceOf, useLive } from "../state/live";
import { canCancel, connectionTone, hasUncommittedWork, isBusy } from "../state/status";
import { tabTarget, useWorkspace, type Tab } from "../state/workspace";
import { Button, Kbd, MenuContent, MenuItem, MenuLabel, StatusDot, cx } from "../ui/primitives";
import { SidebarToggle, WindowEdgeActions, windowEdgeInset, windowLeadingInset } from "./WindowEdgeActions";
import type { HostedWindow } from "./hostedWindow";

const MODE_LABEL: Record<TxnMode, string> = { AutoCommit: "Auto-commit", Manual: "Manual" };
const LOSS_LABEL: Record<TransactionLoss, { label: string; title: string } | null> = {
  None: null,
  RolledBack: { label: "rolled back", title: "Transaction rolled back: connection lost" },
  Unknown: { label: "session reset", title: "Session reset. Earlier commits remain committed; the last transaction outcome may be unknown." },
};

/** Local and an unset environment draw nothing, so a chip always means a shared server. A 6% fill keeps the label at
 * 4.5:1 in L1 Light. */
const ENVIRONMENT_CHIP: Record<Environment, string | null> = {
  Local: null,
  Staging: "bg-amber/6 text-amber",
  Production: "bg-danger/6 text-danger",
};

const TARGET_TRIGGER = "flex h-7 min-w-0 items-center gap-1 rounded-md px-1 text-ui-14 hover:bg-hover data-[state=open]:bg-hover";

/** The databases the tree shows, and the tab's own even when the tree hides it or the connection is down. */
export function databaseChoices(db: DatabaseRef, state: ConnectionState | undefined): string[] {
  const shown = instanceOf(state)?.databases.filter((d) => d.shown).map((d) => d.name) ?? [];
  return shown.includes(db.database) ? shown : [db.database, ...shown];
}

function Separator() {
  return <span className="mx-1.5 shrink-0 text-ui-14 text-faint">/</span>;
}

function DatabaseMenu({ tab, db }: { tab: Tab; db: DatabaseRef }) {
  const state = useLive((s) => s.connections[db.connectionId]);
  return (
    <Menu.Root modal={false}>
      <Menu.Trigger data-testid="tab-database" title={db.database} className={cx(TARGET_TRIGGER, "text-fg")}>
        <span className="truncate">{db.database}</span>
        <ChevronDownIcon size={12} className="shrink-0 text-dim" />
      </Menu.Trigger>
      <MenuContent>
        <MenuLabel>Database for this tab</MenuLabel>
        {databaseChoices(db, state).map((name) => (
          <MenuItem key={name} testId={`tab-database-${name}`} checked={name === db.database} onSelect={() => void setTabTarget(tab.id, { connectionId: db.connectionId, database: name })}>
            {name}
          </MenuItem>
        ))}
      </MenuContent>
    </Menu.Root>
  );
}

export function TopBar({ hosted }: { hosted: HostedWindow | null }) {
  const tab = useWorkspace((s) => (s.active ? s.tabs[s.active] : undefined));
  const configs = useWorkspace((s) => s.configs);
  const layout = useWorkspace((s) => s.layout);
  const db = tabTarget(tab);
  const connectionId = db?.connectionId ?? null;
  const liveConsole = useLive((s) => (tab ? s.consoles[tab.id] : undefined));
  const connections = useLive((s) => s.connections);
  const config = connectionId ? configs[connectionId] : undefined;
  const busy = isBusy(liveConsole);
  const cancellable = canCancel(liveConsole);
  const txn = liveConsole?.txn;
  const txnDepth = liveConsole?.txnDepth;
  const sqlServer = config?.databaseKind === "SqlServer";
  const loss = liveConsole?.session.kind === "Lost" ? LOSS_LABEL[liveConsole.session.transaction] : null;
  const environment = config?.environment ?? null;
  const chip = environment && ENVIRONMENT_CHIP[environment];

  // Each segment after the connection sits in a zero-basis box that gets only the room left over, so the tab title
  // truncates first, then the database, and the connection name last. The boxes clip their own separators, so an
  // empty one takes no room from the connection. The top padding matches the bottom border, so the items stay level
  // with the sidebar header and the traffic lights.
  return (
    <header
      data-tauri-drag-region="deep"
      className={cx(
        "@container flex h-[52px] shrink-0 items-center gap-2 border-b-2 pt-[2px]",
        environment === "Production" ? "border-danger" : "border-transparent",
        windowEdgeInset(hosted),
        layout.sidebar ? "pl-4" : windowLeadingInset(hosted),
      )}
      data-testid="top-bar"
      data-environment={environment ?? undefined}
    >
      {!layout.sidebar && <SidebarToggle />}
      <div className="flex min-w-0 flex-1 items-center">
        {tab ? (
          <Menu.Root modal={false}>
            <Menu.Trigger data-testid="tab-connection" title={config?.name} className={cx(TARGET_TRIGGER, "-ml-1", config ? "text-fg" : "text-dim")}>
              <ConnectionColorDot color={config?.color ?? null} className="mr-0.5" />
              <span className="truncate">{config?.name ?? "No connection"}</span>
              <ChevronDownIcon size={12} className="shrink-0 text-dim" />
            </Menu.Trigger>
            <MenuContent>
              <MenuLabel>Connection for this tab</MenuLabel>
              {Object.values(configs).map((c) => (
                <MenuItem key={c.id} checked={c.id === connectionId} onSelect={() => void setTabTarget(tab.id, homeDb(c))}>
                  <StatusDot tone={connectionTone(connections[c.id])} />
                  <ConnectionColorDot color={c.color} />
                  {c.name}
                </MenuItem>
              ))}
            </MenuContent>
          </Menu.Root>
        ) : (
          <span className="flex min-w-0 items-center gap-1.5 text-ui-14 text-dim">
            <ConnectionColorDot color={config?.color ?? null} />
            <span className="truncate">{config?.name ?? ""}</span>
          </span>
        )}
        {chip && (
          <span data-testid="env-chip" className={cx("ml-1.5 inline-flex h-[18px] shrink-0 items-center rounded-sm px-1.5 text-ui-11.5 leading-none font-medium", chip)}>
            {environment}
          </span>
        )}
        {tab && (
          <div className="flex min-w-0 flex-1 items-center overflow-hidden">
            {db && config && (
              <>
                <Separator />
                <DatabaseMenu tab={tab} db={db} />
              </>
            )}
            <div className="flex min-w-0 flex-1 items-center overflow-hidden">
              <Separator />
              <span data-testid="tab-title" className="truncate text-ui-14 text-muted">{tab.title}</span>
            </div>
          </div>
        )}
      </div>

      {tab && (
        <Menu.Root modal={false}>
          <Menu.Trigger
            data-testid="tx-mode"
            title={loss?.title ?? (txn === "Open" ? "Transaction open. SQL Server does not report nesting depth or committability through the protocol." : undefined)}
            className="inline-flex h-7 items-center gap-1 rounded-md pr-1.5 pl-2 text-ui-12.5 text-muted outline-none hover:bg-hover hover:text-fg data-[state=open]:bg-hover data-[state=open]:text-fg"
          >
            {MODE_LABEL[tab.console.mode]}
            {txn && txn !== "Idle" && (
              <span className={txn === "Aborted" ? "text-danger" : "text-amber"}> · {txn === "Aborted" ? "aborted" : "in transaction"}{txnDepth !== null && txnDepth !== undefined && txnDepth > 1 ? ` (${txnDepth})` : ""}</span>
            )}
            {loss && <span className="text-danger"> · {loss.label}</span>}
            <ChevronDownIcon size={13} />
          </Menu.Trigger>
          <MenuContent align="end">
            {(["AutoCommit", "Manual"] as const).map((m) => (
              <MenuItem key={m} testId={`tx-mode-${m}`} checked={m === tab.console.mode} disabled={sqlServer && m === "Manual"} onSelect={() => void setTabMode(tab.id, m)}>
                {MODE_LABEL[m]}{sqlServer && m === "Manual" ? " (unavailable for SQL Server)" : ""}
              </MenuItem>
            ))}
            {sqlServer && <MenuLabel>Use BEGIN, COMMIT and ROLLBACK in SQL scripts.</MenuLabel>}
          </MenuContent>
        </Menu.Root>
      )}

      {tab && hasUncommittedWork(liveConsole) && (
        <>
          <Button data-testid="commit-btn" title={sqlServer ? "Commit all transaction nesting levels" : undefined} disabled={busy} onClick={() => void endTransaction(tab.id, "commit")}>
            Commit
          </Button>
          <Button data-testid="rollback-btn" disabled={busy} onClick={() => void endTransaction(tab.id, "rollback")}>
            Rollback
          </Button>
        </>
      )}

      {tab && (
        // Below 640px the run target needs the room, and ⌘⌥L still formats.
        <Button data-testid="format-btn" title="Format the selection or the marked statement (⌘⌥L)" className="@max-[640px]:hidden" onClick={() => void formatActive()}>
          <ListIndentIncreaseIcon size={14} className="text-muted" />
          Format
        </Button>
      )}

      {tab && liveConsole?.work === "Count" && <span data-testid="counting-status" role="status" className="text-ui-13 text-dim">Counting…</span>}

      {tab &&
        (cancellable ? (
          <Button data-testid="cancel-btn" title={sqlServer ? "Cancel resets the SQL Server session. Earlier commits remain committed." : undefined} className="w-[104px] justify-center bg-hover" onClick={() => cancelActive()}>
            <SquareIcon fill="currentColor" size={10} className="text-muted" />
            Cancel
            <Kbd className="bg-control text-muted">Esc</Kbd>
          </Button>
        ) : (
          <Button
            data-testid="run-btn"
            disabled={busy}
            variant="primary"
            title="Run the selection or the marked statement (⌘↵). On a blank line, or with ⌘⇧↵, choose which statement to run."
            className="w-[104px] justify-center pr-2 pl-2.5 font-medium"
            onClick={() => void runOrChoose()}
          >
            <PlayIcon fill="currentColor" size={13} className="text-run-icon" />
            Run
            <Kbd className="bg-run-fg/15 text-run-fg/85">⌘↵</Kbd>
          </Button>
        ))}

      {layout.rightPanel === null && (
        <>
          <div className="mx-1 h-5 w-px bg-border" />
          <WindowEdgeActions hosted={hosted} />
        </>
      )}
    </header>
  );
}

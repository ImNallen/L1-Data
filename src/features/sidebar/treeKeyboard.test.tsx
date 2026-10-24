// @vitest-environment jsdom
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, beforeEach, expect, it } from "vitest";
import { buildCatalogView, useCatalog } from "../../state/catalog";
import { dbKey } from "../../state/dbKey";
import { useLive } from "../../state/live";
import { useWorkspace } from "../../state/workspace";
import { connectedState, connectionConfig, databaseState } from "../../testing/connections";
import { shopCatalog } from "../../testing/shopCatalog";
import { dropEditor, seedEditor, textOf } from "../editor/editorStates";
import { Sidebar } from "./Sidebar";

const shop = { connectionId: "p", database: "shop" };

beforeAll(() => {
  Element.prototype.scrollIntoView = () => {};
});

beforeEach(() => {
  mockIPC((cmd) => {
    if (cmd === "table_script") return { text: "SELECT * FROM public.orders", acceptedTexts: [] };
    throw "not served in this test";
  });
  const view = buildCatalogView(shopCatalog("p", 1));
  useWorkspace.setState({ configs: { p: connectionConfig("p", "prod") } });
  useLive.setState({ connections: { p: connectedState("p", [databaseState("shop", true, ["public"])]) } });
  useCatalog.setState({ views: { [dbKey(view.catalog.db)]: view } });
});

afterEach(() => {
  cleanup();
  clearMocks();
  dropEditor("t1");
  useWorkspace.setState(useWorkspace.getInitialState(), true);
  useLive.setState(useLive.getInitialState(), true);
  useCatalog.setState(useCatalog.getInitialState(), true);
});

const row = (path: string) => screen.getByTestId(`tree-node-${path}`);
const focusedPath = () => document.activeElement?.getAttribute("data-testid")?.replace("tree-node-", "");
const tabStops = () => screen.getAllByRole("treeitem").filter((r) => r.tabIndex === 0);

it("is one Tab stop that starts on the first row and returns to the row last focused", async () => {
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  expect(tabStops()).toEqual([row("prod")]);
  expect(row("prod").getAttribute("aria-level")).toBe("1");
  expect(row("prod/shop/public/orders").getAttribute("aria-level")).toBe("4");

  screen.getByTestId("new-connection").focus();
  await user.tab();
  expect(focusedPath()).toBe("prod");

  await user.keyboard("{ArrowDown}{ArrowDown}");
  expect(focusedPath()).toBe("prod/shop/public");
  await user.tab();
  expect(document.activeElement).toBe(screen.getByTestId("open-settings"));
  await user.tab({ shift: true });
  expect(focusedPath()).toBe("prod/shop/public");
  expect(tabStops()).toEqual([row("prod/shop/public")]);
});

it("moves with the arrows, Home and End, and opens and closes rows with Right and Left", async () => {
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  row("prod/shop/public/orders").focus();

  await user.keyboard("{ArrowRight}");
  expect(row("prod/shop/public/orders").getAttribute("aria-expanded")).toBe("true");
  expect(row("prod/shop/public/orders/#columns/status")).toBeTruthy();
  await user.keyboard("{ArrowRight}");
  expect(focusedPath()).toBe("prod/shop/public/orders/#columns");
  await user.keyboard("{ArrowDown}");
  expect(focusedPath()).toBe("prod/shop/public/orders/#columns/id");

  await user.keyboard("{ArrowLeft}");
  expect(focusedPath()).toBe("prod/shop/public/orders/#columns");
  await user.keyboard("{ArrowLeft}");
  expect(screen.queryByTestId("tree-node-prod/shop/public/orders/#columns/id")).toBeNull();
  await user.keyboard("{ArrowLeft}{ArrowLeft}");
  expect(focusedPath()).toBe("prod/shop/public/orders");
  expect(row("prod/shop/public/orders").getAttribute("aria-expanded")).toBe("false");

  await user.keyboard("{End}");
  expect(focusedPath()).toBe("prod/shop/public/#functions");
  await user.keyboard("{Home}");
  expect(focusedPath()).toBe("prod");
  await user.keyboard("{ArrowUp}");
  expect(focusedPath()).toBe("prod");
});

it("opens a table with Enter, as a double-click does", async () => {
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  row("prod/shop/public/orders").focus();
  await user.keyboard("{Enter}");
  await waitFor(() => expect(Object.values(useWorkspace.getState().tabs).map((t) => t.title)).toEqual(["orders"]));
  expect(useWorkspace.getState().selected).toEqual({ db: shop, schema: "public", relation: "orders" });
});

it("inserts a routine's template with Enter, and toggles a row that has no double-click action", async () => {
  useWorkspace.setState({
    tabs: { t1: { id: "t1", title: "Query", console: { db: shop, mode: "AutoCommit" }, table: null, file: null } },
    order: ["t1"],
    active: "t1",
  });
  seedEditor("t1", "");
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  row("prod/shop/public/#functions").focus();

  await user.keyboard("{Enter}");
  expect(row("prod/shop/public/#functions").getAttribute("aria-expanded")).toBe("true");
  await user.keyboard("{ArrowRight}");
  expect(focusedPath()).toBe("prod/shop/public/#functions/apply_coupon");
  await user.keyboard("{Enter}");
  expect(textOf("t1")).toContain("apply_coupon()");
});

it("walks the search results the same way, entered from the search box with Down", async () => {
  useWorkspace.setState({ treeQuery: "order" });
  const user = userEvent.setup();
  render(<Sidebar hosted={null} />);
  screen.getByTestId("sidebar-search").focus();

  await user.keyboard("{ArrowDown}");
  expect(focusedPath()).toBe("prod");
  await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}");
  expect(focusedPath()).toBe("prod/shop/public/order_items");
  await user.keyboard("{ArrowLeft}");
  expect(screen.queryByTestId("tree-node-prod/shop/public/order_items/#columns/order_id")).toBeNull();
  await user.keyboard("{ArrowLeft}");
  expect(focusedPath()).toBe("prod/shop/public");
  await user.keyboard("{ArrowLeft}");
  expect(screen.queryByTestId("tree-node-prod/shop/public/orders")).toBeNull();
  expect(row("prod/shop/public").getAttribute("aria-expanded")).toBe("false");
});

it("marks a Production or Staging connection's row and leaves a Local one plain", () => {
  useWorkspace.setState({
    configs: {
      p: { ...connectionConfig("p", "prod"), environment: "Production" },
      s: { ...connectionConfig("s", "stage"), environment: "Staging" },
      l: { ...connectionConfig("l", "local"), environment: "Local" },
    },
  });
  render(<Sidebar hosted={null} />);
  expect(screen.getByTestId("env-chip-prod").textContent).toBe("Prod");
  expect(screen.getByTestId("env-chip-prod").title).toBe("Production");
  expect(screen.getByTestId("env-chip-stage").textContent).toBe("Staging");
  expect(screen.queryByTestId("env-chip-local")).toBeNull();
});

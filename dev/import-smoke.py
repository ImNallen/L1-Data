#!/usr/bin/env python3
"""Verify CSV import against a running bridge and PostgreSQL. Use a release bridge for --perf."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time
import urllib.request
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bridge", default="http://127.0.0.1:7387")
    parser.add_argument("--url", default=os.getenv("L1_DATA_TEST_PG", "postgres://l1data:l1data@localhost:5433/shop"))
    parser.add_argument("--perf", action="store_true")
    parser.add_argument("--ui-session", help="Time the actual Import click through a named agent-browser session already open on this bridge")
    parser.add_argument("--output", type=Path, default=Path("/tmp/p12-review/import-smoke.json"))
    args = parser.parse_args()
    from urllib.parse import urlparse
    url = urlparse(args.url)

    def reply(command, **body):
        request = urllib.request.Request(f"{args.bridge}/invoke/{command}", json.dumps(body).encode(), {"Content-Type": "application/json"})
        with urllib.request.urlopen(request, timeout=60) as response:
            return json.load(response)

    def invoke(command, **body):
        result = reply(command, **body)
        assert result["status"] == "ok", (command, result)
        return result.get("data")

    def sql(statement):
        return subprocess.run(["psql", args.url, "-X", "-v", "ON_ERROR_STOP=1", "-Atq", "-c", statement], check=True, capture_output=True, text=True).stdout.strip()

    token = uuid.uuid4().hex[:12]
    tables, tabs = [], []
    connection = invoke("connection_save", draft={"id": None, "databaseKind": "Postgres", "name": f"P12 smoke {token}", "host": url.hostname, "port": url.port or 5432, "database": url.path[1:], "user": url.username, "sslMode": "Prefer", "caCertPath": None, "password": {"Set": url.password}, "color": None, "environment": None, "ssh": None, "sqlServerAuth": None})
    conn = connection["id"]
    db = {"connectionId": conn, "database": url.path[1:]}
    invoke("connection_connect", id=conn)
    evidence = {"bridge": args.bridge, "timing": "Import click to rendered success toast" if args.ui_session else "table_import request to completed reply", "cases": [], "runs": []}

    def table(suffix, columns="n integer, txt text, fallback text DEFAULT 'default'"):
        name = f"p12_{token}_{suffix}"
        sql(f"CREATE TABLE {name} ({columns})")
        tables.append(name)
        invoke("catalog_refresh", db=db)
        catalog = invoke("catalog_get", db=db)
        relation = next(r for s in catalog["schemas"] if s["name"] == "public" for r in s["relations"] if r["name"] == name)
        return {"db": db, "schema": "public", "table": name, "oid": relation["oid"]}

    def tab(mode="AutoCommit"):
        tab_id = str(uuid.uuid4())
        tabs.append(tab_id)
        invoke("console_configure", tabId=tab_id, config={**db, "schema": None, "mode": mode})
        return tab_id

    def prepare(tab_id, target, path, header=True):
        started = time.perf_counter()
        source = invoke("table_import_prepare", tabId=tab_id, target=target, file=str(path))
        preview = invoke("table_import_preview", tabId=tab_id, source=source, header=header)
        return {"source": source, "header": header, "mapping": preview["mapping"]}, time.perf_counter() - started

    def browser_eval(code):
        result = subprocess.run(["agent-browser", "--session", args.ui_session, "eval", code], check=True, capture_output=True, text=True)
        return json.loads(result.stdout)

    def ui_import(target, path):
        row = f"tree-node-{connection['name']}/{db['database']}/public/{target['table']}"
        subprocess.run(["agent-browser", "--session", args.ui_session, "wait", f'[data-testid="{row}"]'], check=True, capture_output=True)
        browser_eval(f"window.__l1Dialog.answer({json.dumps(str(path))}); document.querySelector({json.dumps('[data-testid=' + json.dumps(row) + ']')}).dispatchEvent(new MouseEvent('contextmenu', {{bubbles:true,clientX:180,clientY:250}}))")
        def click_until(button, condition):
            return browser_eval("(async () => { const start=performance.now(); const done=new Promise((resolve,reject)=>{const timer=setTimeout(()=>{observer.disconnect();reject(new Error('UI timeout'));},30000);const observer=new MutationObserver(()=>{if(" + condition + "){clearTimeout(timer);observer.disconnect();resolve((performance.now()-start)/1000);}});observer.observe(document.body,{subtree:true,childList:true,attributes:true,characterData:true});});document.querySelector('[data-testid=\"" + button + "\"]').click();return await done;})()")
        preparation = click_until("tree-menu-import", "document.querySelector('[data-testid=\"import-submit\"]')")
        until = time.monotonic() + 10
        while browser_eval("document.querySelector('[data-testid=\"toast\"]') !== null"):
            assert time.monotonic() < until, "Previous toast did not clear"
            time.sleep(0.05)
        elapsed = click_until("import-submit", "document.querySelector('[data-testid=\"toast\"]')?.textContent === '100,000 rows imported.'")
        return preparation, elapsed

    def run(tab_id, request):
        return invoke("table_import", tabId=tab_id, request=request, confirmed=None)

    def count(target):
        return int(sql(f"SELECT count(*) FROM {target['table']}"))

    if args.ui_session:
        subprocess.run(["agent-browser", "--session", args.ui_session, "reload"], check=True, capture_output=True)
    try:
        with tempfile.TemporaryDirectory(prefix="p12-csv-") as directory:
            path = Path(directory) / "source.csv"
            target = table("clean")
            tab_id = tab()
            path.write_text("txt,n\n" + "".join(f'"row,{n}",{n}\n' for n in range(1000)))
            request, _ = prepare(tab_id, target, path)
            path.write_text("n,txt\n999999,replaced\n")
            assert run(tab_id, request)["rows"] == 1000 and count(target) == 1000
            assert reply("table_import", tabId=tab_id, request=request, confirmed=None)["error"]["kind"] == "Gone"
            assert sql(f"SELECT count(*) FROM {target['table']} WHERE fallback='default' AND txt='row,' || n") == "1000"
            evidence["cases"].append("1000 rows, swapped header, defaults, immutable source, single use")
            path.write_text("n,txt\n1,a\nwrong,b\n")
            request, _ = prepare(tab_id, target, path)
            error = reply("table_import", tabId=tab_id, request=request, confirmed=None)
            assert error["error"]["kind"] == "Query" and count(target) == 1000
            evidence["cases"].append({"bad_row_atomic": error["error"]})
            path.write_text('n,txt\n1,\n2,""\n3,"a,b\n雪"\n')
            manual = tab("Manual")
            request, _ = prepare(manual, target, path)
            assert not run(manual, request)["committed"] and count(target) == 1000
            invoke("console_rollback", tabId=manual)
            assert count(target) == 1000
            request, _ = prepare(tab_id, target, path)
            assert run(tab_id, request)["rows"] == 3
            assert sql(f"SELECT count(*) FROM {target['table']} WHERE txt IS NULL") == "1"
            assert sql(f"SELECT count(*) FROM {target['table']} WHERE txt = ''") == "1"
            evidence["cases"].append("Manual rollback, NULL, quoted empty, multiline Unicode")
            if args.perf:
                path.write_text("n,txt\n" + "".join(f'{n},row-{n:06d}\n' for n in range(100_000)))
                for index in range(5):
                    baseline = table(f"psql_{index}")
                    started = time.perf_counter()
                    sql(f"\\copy {baseline['table']} (n,txt) FROM '{path}' CSV HEADER")
                    psql_seconds = time.perf_counter() - started
                    assert count(baseline) == 100_000
                    imported = table(f"app_{index}")
                    fresh_tab = tab()
                    if args.ui_session:
                        preparation, app_seconds = ui_import(imported, path)
                    else:
                        request, preparation = prepare(fresh_tab, imported, path)
                        started = time.perf_counter()
                        result = run(fresh_tab, request)
                        app_seconds = time.perf_counter() - started
                        assert result["rows"] == 100_000 and result["committed"]
                    assert count(imported) == 100_000
                    evidence["runs"].append({"psql_seconds": psql_seconds, "prepare_seconds": preparation, "import_seconds": app_seconds, "rows": 100_000, "bytes": path.stat().st_size})
                evidence["medians"] = {key: statistics.median(r[key] for r in evidence["runs"]) for key in ("psql_seconds", "prepare_seconds", "import_seconds")}
                median = evidence["medians"]
                evidence["roadmap_gate"] = median["import_seconds"] <= 5 and median["import_seconds"] <= median["psql_seconds"] * 1.25
            evidence["errors"] = 0
    finally:
        for console in invoke("app_boot")["consoles"]:
            if console["config"]["connectionId"] == conn and console["tabId"] not in tabs:
                tabs.append(console["tabId"])
        for tab_id in tabs:
            invoke("console_close", tabId=tab_id, force=True)
        invoke("connection_disconnect", id=conn, force=True)
        invoke("connection_delete", id=conn)
        for name in tables:
            sql(f"DROP TABLE IF EXISTS {name}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps(evidence, indent=2))


if __name__ == "__main__":
    main()

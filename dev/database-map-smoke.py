#!/usr/bin/env python3
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
from urllib.request import Request, urlopen


port = int(os.environ.get("L1_DATA_BRIDGE_PORT", "7277"))
with socket.socket() as probe:
    probe.bind(("127.0.0.1", port))
base = f"http://127.0.0.1:{port}"


def response(command, **arguments):
    request = Request(f"{base}/invoke/{command}", json.dumps(arguments).encode(), {"content-type": "application/json"})
    with urlopen(request, timeout=20) as reply:
        return json.load(reply)


def invoke(command, **arguments):
    reply = response(command, **arguments)
    assert reply["status"] == "ok", (command, reply)
    return reply["data"]


def quoted(name, kind):
    return '[' + name.replace(']', ']]') + ']' if kind == 'SqlServer' else '"' + name.replace('"', '""') + '"'


with tempfile.TemporaryDirectory(prefix="l1-map-") as directory:
    with open(Path(directory) / "bridge.log", "w+") as log:
        process = subprocess.Popen(["./target/debug/dev-bridge"], stdout=log, stderr=log, env={
            **os.environ, "L1_DATA_DATA_DIR": directory, "L1_DATA_BRIDGE_PORT": str(port),
        })
        try:
            deadline = time.monotonic() + 15
            while True:
                try:
                    invoke("app_boot")
                    break
                except OSError:
                    if process.poll() is not None or time.monotonic() > deadline:
                        log.seek(0)
                        raise AssertionError(log.read())
                    time.sleep(0.05)
            for kind, schema, user, password, db_port in [
                ("Postgres", "public", "l1data", "l1data", 5433),
                ("SqlServer", "dbo", "sa", os.environ.get("L1_DATA_TEST_MSSQL_PASSWORD", "L1Data_dev_Str0ng"), int(os.environ.get("L1_DATA_TEST_MSSQL_PORT", "1434"))),
            ]:
                config = invoke("connection_save", draft={
                    "id": None, "name": f"Map smoke {kind}", "databaseKind": kind, "host": "localhost", "port": db_port,
                    "database": "shop", "user": user, "password": {"Set": password}, "sslMode": "Require" if kind == "SqlServer" else "Prefer",
                    "caCertPath": None, "color": None, "ssh": None, "environment": None, "sqlServerAuth": None,
                })
                invoke("connection_connect", id=config["id"])
                db = {"connectionId": config["id"], "database": "shop"}
                catalog = invoke("catalog_get", db=db)
                root = next(r for s in catalog["schemas"] if s["name"] == schema for r in s["relations"] if r["name"] == "orders")
                args = {"db": db, "oid": root["oid"], "catalogVersion": catalog["version"], "after": None}
                page = invoke("database_map_neighborhood", **args)
                assert page["root"]["oid"] == root["oid"] and page["foreignKeys"], page
                key = next(k for k in page["foreignKeys"] if k["source"] == root["oid"])
                target = next(r for r in page["relations"] if r["oid"] == key["target"])
                pair = key["columns"][0]
                table = lambda s, n: f"{quoted(s, kind)}.{quoted(n, kind)}"
                text = f"SELECT '😀' FROM {table(schema, root['name'])} a JOIN {table(target['schema'], target['name'])} b ON a.{quoted(pair['source'], kind)}=b.{quoted(pair['target'], kind)}"
                analysis = invoke("database_map_query", db=db, text=text, cursor=len(text.encode("utf-16-le")) // 2, catalogVersion=catalog["version"])
                assert len(analysis["occurrences"]) == 2 and len(analysis["joins"]) == 1 and not analysis["incomplete"], analysis
                assert all(o["resolution"]["kind"] == "CatalogMatch" for o in analysis["occurrences"]), analysis
                def source_range(span):
                    return text.encode("utf-16-le")[span["from"] * 2:span["to"] * 2].decode("utf-16-le")
                assert [source_range(o["range"]) for o in analysis["occurrences"]] == [table(schema, root["name"]), table(target["schema"], target["name"])], analysis
                assert source_range(analysis["joins"][0]["range"]) == f"a.{quoted(pair['source'], kind)}=b.{quoted(pair['target'], kind)}", analysis
                assert response("database_map_neighborhood", **{**args, "catalogVersion": catalog["version"] + 1})["error"]["kind"] == "Stale"
                assert response("database_map_neighborhood", **{**args, "oid": 0})["error"]["kind"] == "Gone"
                invoke("connection_disconnect", id=config["id"], force=True)
                assert response("database_map_neighborhood", **args)["error"]["kind"] == "Catalog"
                print(f"{kind}: neighborhood, directed column pairs, UTF-16 query ranges, stale epoch, Gone and disconnect passed")
        finally:
            process.terminate()
            try:
                process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()

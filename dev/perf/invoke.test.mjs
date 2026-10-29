import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { parseOptions, percentile } from "./invoke.mjs";

const script = fileURLToPath(new URL("./invoke.mjs", import.meta.url));

const probe = (args) =>
  new Promise((resolve) => execFile(process.execPath, [script, ...args], (error, stdout, stderr) => resolve({ code: error?.code ?? 0, stdout, stderr })));

function afterMinimumDelay(delay, callback) {
  const deadline = performance.now() + delay;
  const fire = () => {
    const remaining = deadline - performance.now();
    if (remaining > 0) setTimeout(fire, remaining);
    else callback();
  };
  setTimeout(fire, delay);
}

async function fakeBridge({ finishAfterMs = 30, outcome = "Ok", replyAfterMs = 0 } = {}) {
  const calls = [];
  const streams = new Set();
  let nextRun = 1;
  const server = createServer((req, res) => {
    if (req.url === "/events") {
      res.writeHead(200, { "content-type": "text/event-stream" }).flushHeaders();
      streams.add(res);
      return;
    }
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      const command = req.url.replace("/invoke/", "");
      calls.push({ command, body });
      const runId = nextRun++;
      afterMinimumDelay(replyAfterMs, () => res.end(JSON.stringify({ status: "ok", data: command === "console_run" ? { runId } : null })));
      if (command !== "console_run" || finishAfterMs === null) return;
      afterMinimumDelay(finishAfterMs, () => {
        const event = { kind: "ConsoleChanged", run: { kind: "Finished", runId, outcome } };
        for (const stream of streams) stream.write(`data:${JSON.stringify(event)}\n\n`);
      });
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return {
    url: `http://127.0.0.1:${server.address().port}`,
    calls,
    close: () => {
      for (const stream of streams) stream.destroy();
      server.close();
    },
  };
}

test("the original five-argument form sends one body to both sides", () => {
  const opts = parseOptions(["http://a/", "http://b", "connection_list", "{}", "3"]);
  assert.deepEqual(opts, { bridges: ["http://a", "http://b"], command: "connection_list", runs: 3, bodies: ["{}", "{}"], wait: null, timeoutMs: 30000, teardown: null, raw: null });
});

test("per-side bodies replace the shared body", () => {
  const opts = parseOptions(["a", "b", "console_run", "4", "--body-a", "@a.json", "--body-b", "@b.json", "--wait", "timed"]);
  assert.deepEqual(opts.bodies, ["@a.json", "@b.json"]);
  assert.equal(opts.runs, 4);
  assert.equal(opts.wait, "timed");
  assert.equal(parseOptions(["a", "b", "c", "1", "--body-a", "{}", "--body-b", "{}", "--wait", "untimed", "--timeout-ms", "500"]).timeoutMs, 500);
  assert.deepEqual(parseOptions(["a", "b", "c", "{}", "1", "--body-b", "{\"x\":1}"]).bodies, ["{}", "{\"x\":1}"]);
});

test("a teardown takes a shared body or one per side", () => {
  const opts = parseOptions(["a", "b", "connection_connect", "2", "--body-a", "A", "--body-b", "B", "--teardown", "connection_disconnect", "--teardown-body", "T", "--teardown-body-b", "TB"]);
  assert.deepEqual(opts.teardown, { command: "connection_disconnect", bodies: ["T", "TB"] });
});

test("bad arguments are refused", () => {
  const refused = [
    ["a", "b", "c", "{}"],
    ["a", "b", "c", "{}", "0"],
    ["a", "b", "c", "{}", "1.5"],
    ["a", "b", "c", "2", "--body-a", "{}"],
    ["a", "b", "c", "{}", "2", "--wait", "soon"],
    ["a", "b", "c", "{}", "2", "--teardown", "d"],
    ["a", "b", "c", "{}", "2", "--teardown-body", "{}"],
    ["a", "b", "c", "{}", "2", "--wait", "timed", "--timeout-ms", "0"],
    ["a", "b", "c", "{}", "2", "--wait", "timed", "--timeout-ms", "soon"],
    ["a", "b", "c", "{}", "2", "--timeout-ms", "500"],
    ["a", "b", "c", "{}", "2", "--body"],
    ["a", "b", "c", "{}", "2", "--bogus", "x"],
  ];
  for (const argv of refused) assert.throws(() => parseOptions(argv), undefined, argv.join(" "));
});

test("percentiles use the nearest rank", () => {
  const twenty = Array.from({ length: 20 }, (_, i) => i + 1);
  assert.equal(percentile(twenty, 0.5), 10);
  assert.equal(percentile(twenty, 0.95), 19);
  assert.equal(percentile([1, 2, 3], 0.5), 2);
  assert.equal(percentile([7], 0.95), 7);
});

test("a timed wait measures each run to its Finished event, tears down after each, and keeps every sample", async (t) => {
  const finishAfterMs = 30;
  const schedule = globalThis.setTimeout;
  t.mock.method(globalThis, "setTimeout", (callback, delay, ...args) => schedule(callback, delay === finishAfterMs ? 0 : delay, ...args));
  const [a, b] = await Promise.all([fakeBridge({ finishAfterMs }), fakeBridge({ finishAfterMs })]);
  const raw = join(mkdtempSync(join(tmpdir(), "invoke-")), "raw.json");
  try {
    const { code, stdout, stderr } = await probe([
      a.url,
      b.url,
      "console_run",
      "3",
      ...["--body-a", '{"tab":"A"}', "--body-b", '{"tab":"B"}', "--wait", "timed", "--raw", raw],
      ...["--teardown", "console_close", "--teardown-body-a", '{"close":"A"}', "--teardown-body-b", '{"close":"B"}'],
    ]);
    assert.equal(code, 0, stderr);
    const summary = JSON.parse(stdout);
    const saved = JSON.parse(readFileSync(raw, "utf8"));
    for (const [side, bridge, tab] of [["a", a, "A"], ["b", b, "B"]]) {
      const cycle = [
        { command: "console_run", body: `{"tab":"${tab}"}` },
        { command: "console_close", body: `{"close":"${tab}"}` },
      ];
      assert.deepEqual(bridge.calls, Array(4).fill(cycle).flat(), `${side} ran warm-up plus three cycles with its own bodies`);
      assert.equal(saved[side].samplesMs.length, 3);
      assert.ok(saved[side].samplesMs.every((ms) => ms >= finishAfterMs), `${side} samples include the wait: ${saved[side].samplesMs}`);
      assert.equal(saved[side].p50Ms, percentile(saved[side].samplesMs.toSorted((x, y) => x - y), 0.5));
      assert.deepEqual(summary[side], { url: bridge.url, p50Ms: saved[side].p50Ms, p95Ms: saved[side].p95Ms });
    }
    assert.equal(summary.wait, "timed");
    assert.equal(summary.teardown, "console_close");
  } finally {
    a.close();
    b.close();
  }
});

test("an untimed wait holds each call until its run finishes but measures only the call", async () => {
  const [a, b] = await Promise.all([fakeBridge({ finishAfterMs: 200 }), fakeBridge({ finishAfterMs: 200 })]);
  try {
    const started = performance.now();
    const { code, stdout, stderr } = await probe([a.url, b.url, "console_run", "{}", "2", "--wait", "untimed"]);
    assert.equal(code, 0, stderr);
    assert.ok(performance.now() - started >= 6 * 200, "six runs finished one after another");
    const summary = JSON.parse(stdout);
    assert.ok(summary.a.p95Ms < 200 && summary.b.p95Ms < 200, stdout);
  } finally {
    a.close();
    b.close();
  }
});

test("a run that does not finish Ok fails the probe", async () => {
  const [a, b] = await Promise.all([fakeBridge({ outcome: "Failed" }), fakeBridge()]);
  try {
    const { code, stderr } = await probe([a.url, b.url, "console_run", "{}", "2", "--wait", "untimed"]);
    assert.equal(code, 1);
    assert.match(stderr, /finished Failed/);
  } finally {
    a.close();
    b.close();
  }
});

test("a timed wait never ends before the call returns", async () => {
  const [a, b] = await Promise.all([fakeBridge({ finishAfterMs: 0, replyAfterMs: 100 }), fakeBridge({ finishAfterMs: 0, replyAfterMs: 100 })]);
  try {
    const { code, stdout, stderr } = await probe([a.url, b.url, "console_run", "{}", "2", "--wait", "timed"]);
    assert.equal(code, 0, stderr);
    const summary = JSON.parse(stdout);
    assert.ok(summary.a.p50Ms >= 100 && summary.b.p50Ms >= 100, stdout);
  } finally {
    a.close();
    b.close();
  }
});

test("a run that never finishes fails the probe once the timeout passes, naming the run", async () => {
  const [a, b] = await Promise.all([fakeBridge({ finishAfterMs: null }), fakeBridge()]);
  try {
    const started = performance.now();
    const { code, stderr } = await probe([a.url, b.url, "console_run", "{}", "2", "--wait", "untimed", "--timeout-ms", "300"]);
    assert.equal(code, 1);
    assert.match(stderr, /run 1 did not finish within 300 ms/);
    assert.ok(performance.now() - started < 5000, "the probe gave up instead of hanging");
  } finally {
    a.close();
    b.close();
  }
});

test("a shared body passed with both per-side bodies is ignored with a warning", async () => {
  const [a, b] = await Promise.all([fakeBridge(), fakeBridge()]);
  try {
    const { code, stderr } = await probe([a.url, b.url, "noop", '{"shared":1}', "1", "--body-a", '{"side":"A"}', "--body-b", '{"side":"B"}']);
    assert.equal(code, 0, stderr);
    assert.match(stderr, /the shared body is ignored/);
    assert.ok(a.calls.every((call) => call.body === '{"side":"A"}') && b.calls.every((call) => call.body === '{"side":"B"}'));
  } finally {
    a.close();
    b.close();
  }
});

test("bad arguments exit 2 with the usage", async () => {
  const { code, stderr } = await probe(["http://a", "http://b", "console_run", "{}", "2", "--wait", "soon"]);
  assert.equal(code, 2);
  assert.match(stderr, /--wait takes untimed or timed/);
  assert.match(stderr, /^usage: node dev\/perf\/invoke.mjs/m);
});

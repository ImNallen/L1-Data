#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const USAGE = `usage: node dev/perf/invoke.mjs <bridge-a> <bridge-b> <command> [<json | @file>] <runs> [options]

  --body-a, --body-b <json | @file>   the side's request body, in place of the shared one
  --wait untimed | timed              after each call, wait for the run's Finished event and fail
                                      unless it is Ok. timed measures from the call to the later
                                      of the reply and the event.
  --timeout-ms <ms>                   fail when a run has not finished this long after its call
                                      returns, default 30000
  --teardown <command>                call after each sample, untimed
  --teardown-body <json | @file>      its body, or per side with --teardown-body-a, --teardown-body-b
  --raw <path>                        write every sample to a JSON file`;

const OPTIONS = {
  "body-a": { type: "string" },
  "body-b": { type: "string" },
  wait: { type: "string" },
  "timeout-ms": { type: "string" },
  teardown: { type: "string" },
  "teardown-body": { type: "string" },
  "teardown-body-a": { type: "string" },
  "teardown-body-b": { type: "string" },
  raw: { type: "string" },
};

export function parseOptions(argv) {
  const { values, positionals } = parseArgs({ args: argv, options: OPTIONS, allowPositionals: true });
  if (positionals.length !== 4 && positionals.length !== 5) throw new Error("expected 4 or 5 positional arguments");
  const [urlA, urlB, command] = positionals;
  const body = positionals.length === 5 ? positionals[3] : undefined;
  const runs = Number(positionals.at(-1));
  if (!Number.isInteger(runs) || runs < 1) throw new Error(`runs must be a positive integer, not ${positionals.at(-1)}`);
  if (values.wait !== undefined && values.wait !== "untimed" && values.wait !== "timed") throw new Error("--wait takes untimed or timed");
  const timeoutMs = Number(values["timeout-ms"] ?? 30000);
  if (!Number.isInteger(timeoutMs) || timeoutMs < 1) throw new Error(`--timeout-ms must be a positive integer, not ${values["timeout-ms"]}`);
  if (values["timeout-ms"] !== undefined && values.wait === undefined) throw new Error("--timeout-ms needs --wait");
  const bodies = [values["body-a"] ?? body, values["body-b"] ?? body];
  if (bodies.includes(undefined)) throw new Error("each side needs a body");
  if (body !== undefined && values["body-a"] !== undefined && values["body-b"] !== undefined) {
    console.error("warning: --body-a and --body-b are both set, so the shared body is ignored");
  }
  let teardown = null;
  if (values.teardown !== undefined) {
    const shared = values["teardown-body"];
    teardown = { command: values.teardown, bodies: [values["teardown-body-a"] ?? shared, values["teardown-body-b"] ?? shared] };
    if (teardown.bodies.includes(undefined)) throw new Error("each side needs a teardown body");
  } else if (["teardown-body", "teardown-body-a", "teardown-body-b"].some((key) => values[key] !== undefined)) {
    throw new Error("a teardown body needs --teardown");
  }
  return {
    bridges: [urlA, urlB].map((url) => url.replace(/\/+$/, "")),
    command,
    runs,
    bodies,
    wait: values.wait ?? null,
    timeoutMs,
    teardown,
    raw: values.raw ?? null,
  };
}

export function percentile(sorted, p) {
  return sorted[Math.max(0, Math.ceil(p * sorted.length) - 1)];
}

const load = (spec) => (spec.startsWith("@") ? readFileSync(spec.slice(1), "utf8") : spec);
const round = (ms) => Math.round(ms * 1000) / 1000;

async function invoke(bridge, command, body) {
  const res = await fetch(`${bridge}/invoke/${command}`, { method: "POST", headers: { "content-type": "application/json" }, body });
  const reply = await res.json();
  if (res.status !== 200 || reply.status !== "ok") throw new Error(`${bridge} ${command}: HTTP ${res.status} ${JSON.stringify(reply)}`);
  return reply.data;
}

async function watchRuns(bridge, signal) {
  const runs = new Map();
  let failure = null;
  const run = (runId) => {
    if (!runs.has(runId)) {
      const entry = {};
      entry.finished = new Promise((resolve, reject) => Object.assign(entry, { resolve, reject }));
      if (failure) entry.reject(failure);
      runs.set(runId, entry);
    }
    return runs.get(runId);
  };
  const res = await fetch(`${bridge}/events`, { signal });
  if (res.status !== 200) throw new Error(`${bridge}/events: HTTP ${res.status}`);
  (async () => {
    const decoder = new TextDecoder();
    let buffer = "";
    for await (const chunk of res.body) {
      buffer += decoder.decode(chunk, { stream: true });
      let cut;
      while ((cut = buffer.indexOf("\n\n")) >= 0) {
        const data = buffer
          .slice(0, cut)
          .split("\n")
          .filter((line) => line.startsWith("data:"))
          .map((line) => line.slice(5))
          .join("\n");
        buffer = buffer.slice(cut + 2);
        if (!data) continue;
        const event = JSON.parse(data);
        if (event.kind === "ConsoleChanged" && event.run.kind === "Finished") {
          run(event.run.runId).resolve({ at: performance.now(), outcome: event.run.outcome });
        }
      }
    }
    throw new Error(`${bridge}/events ended`);
  })().catch((error) => {
    failure = error;
    for (const entry of runs.values()) entry.reject(failure);
  });
  return (runId) => run(runId).finished;
}

async function finishedWithin(finished, runId, timeoutMs, bridge) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${bridge} run ${runId} did not finish within ${timeoutMs} ms`)), timeoutMs);
  });
  try {
    return await Promise.race([finished(runId), timeout]);
  } finally {
    clearTimeout(timer);
  }
}

async function sample(side, opts, finished) {
  const started = performance.now();
  const reply = await invoke(side.bridge, opts.command, side.body);
  const replied = performance.now();
  let ms = replied - started;
  if (finished) {
    if (reply?.runId === undefined) throw new Error(`${side.bridge} ${opts.command}: --wait needs a reply with a runId, got ${JSON.stringify(reply)}`);
    const { at, outcome } = await finishedWithin(finished, reply.runId, opts.timeoutMs, side.bridge);
    if (outcome !== "Ok") throw new Error(`${side.bridge} run ${reply.runId} finished ${outcome}`);
    if (opts.wait === "timed") ms = Math.max(replied, at) - started;
  }
  if (opts.teardown) await invoke(side.bridge, opts.teardown.command, side.teardownBody);
  return ms;
}

async function main(opts) {
  const sides = opts.bridges.map((bridge, i) => ({
    bridge,
    body: load(opts.bodies[i]),
    teardownBody: opts.teardown && load(opts.teardown.bodies[i]),
    samples: [],
  }));
  const events = new AbortController();
  const watchers = opts.wait ? await Promise.all(sides.map((side) => watchRuns(side.bridge, events.signal))) : [null, null];
  for (const i of [0, 1]) await sample(sides[i], opts, watchers[i]);
  for (let run = 0; run < opts.runs; run++) {
    for (const i of run % 2 === 0 ? [0, 1] : [1, 0]) sides[i].samples.push(await sample(sides[i], opts, watchers[i]));
  }
  events.abort();
  const summary = (side) => {
    const sorted = side.samples.toSorted((x, y) => x - y);
    return { url: side.bridge, p50Ms: round(percentile(sorted, 0.5)), p95Ms: round(percentile(sorted, 0.95)) };
  };
  const result = { command: opts.command, runs: opts.runs, wait: opts.wait, teardown: opts.teardown?.command ?? null, a: summary(sides[0]), b: summary(sides[1]) };
  if (opts.raw) {
    const raw = (side, s) => ({ ...s, samplesMs: side.samples.map(round) });
    writeFileSync(opts.raw, `${JSON.stringify({ ...result, a: raw(sides[0], result.a), b: raw(sides[1], result.b) }, null, 2)}\n`);
  }
  console.log(JSON.stringify(result, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  let opts;
  try {
    opts = parseOptions(process.argv.slice(2));
  } catch (error) {
    console.error(`${error.message}\n\n${USAGE}`);
    process.exit(2);
  }
  await main(opts);
}

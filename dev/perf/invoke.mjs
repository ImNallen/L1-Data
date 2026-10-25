#!/usr/bin/env node
import { readFileSync } from "node:fs";

const [urlA, urlB, command, bodyArg, runsArg] = process.argv.slice(2);
const runs = Number(runsArg);
if (!command || bodyArg === undefined || !Number.isInteger(runs) || runs < 1) {
  console.error("usage: node dev/perf/invoke.mjs <bridge-a> <bridge-b> <command> <json | @file> <runs>");
  process.exit(2);
}
const body = bodyArg.startsWith("@") ? readFileSync(bodyArg.slice(1), "utf8") : bodyArg;
const bridges = [urlA, urlB].map((url) => url.replace(/\/+$/, ""));

async function call(bridge) {
  const started = performance.now();
  const res = await fetch(`${bridge}/invoke/${command}`, { method: "POST", headers: { "content-type": "application/json" }, body });
  const reply = await res.json();
  const ms = performance.now() - started;
  if (res.status !== 200 || reply.status !== "ok") throw new Error(`${bridge} ${command}: HTTP ${res.status} ${JSON.stringify(reply)}`);
  return ms;
}

function percentile(sorted, p) {
  return sorted[Math.max(0, Math.ceil(p * sorted.length) - 1)];
}

function summary(url, samples) {
  const sorted = samples.toSorted((x, y) => x - y);
  const round = (ms) => Math.round(ms * 1000) / 1000;
  return { url, p50Ms: round(percentile(sorted, 0.5)), p95Ms: round(percentile(sorted, 0.95)) };
}

async function warmUp() {
  for (const bridge of bridges) await call(bridge);
}

await warmUp();
const samples = [[], []];
for (let i = 0; i < runs; i++) {
  for (const side of i % 2 === 0 ? [0, 1] : [1, 0]) samples[side].push(await call(bridges[side]));
}
console.log(JSON.stringify({ command, runs, a: summary(bridges[0], samples[0]), b: summary(bridges[1], samples[1]) }, null, 2));

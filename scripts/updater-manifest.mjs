import { createHash, createPublicKey, verify } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { pathToFileURL } from "node:url";
import { compareVersions, parseReleases, parseVersion } from "./release-plan.mjs";

export const bundles = [
  { os: "macOS", suffix: ".app.tar.gz", asset: (v) => `L1-Data_${v}_universal.app.tar.gz`, platforms: ["darwin-aarch64", "darwin-x86_64"] },
  { os: "macOS", suffix: ".dmg", asset: (v) => `L1-Data_${v}_universal.dmg`, platforms: [] },
  { os: "Linux", suffix: ".AppImage", asset: (v) => `L1-Data_${v}_amd64.AppImage`, platforms: ["linux-x86_64"] },
  { os: "Linux", suffix: ".deb", asset: (v) => `L1-Data_${v}_amd64.deb`, platforms: [] },
  { os: "Windows", suffix: "-setup.exe", asset: (v) => `L1-Data_${v}_x64-setup.exe`, platforms: ["windows-x86_64"] },
];

export function stagePlan(os, version, artifactPaths) {
  const ours = bundles.filter((bundle) => bundle.os === os);
  if (ours.length === 0) throw new Error(`No bundles are built on ${os}.`);
  return ours.flatMap((bundle) => {
    const found = artifactPaths.filter((path) => path.endsWith(bundle.suffix));
    if (found.length !== 1) throw new Error(`Expected one ${bundle.suffix} bundle on ${os}, found ${found.length}: ${found.join(", ")}`);
    const copy = { from: found[0], to: bundle.asset(version) };
    return bundle.platforms.length ? [copy, { from: `${copy.from}.sig`, to: `${copy.to}.sig` }] : [copy];
  });
}

const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");
const lines = (base64) => Buffer.from(base64.trim(), "base64").toString("utf8").trim().split(/\r?\n/);
const keyId = (bytes) => Buffer.from(bytes).reverse().toString("hex").toUpperCase();

export function verifySignature(publicKey, signature, data) {
  const pk = Buffer.from(lines(publicKey)[1], "base64");
  const [, sigLine, trustedLine, globalLine] = lines(signature);
  const sig = Buffer.from(sigLine, "base64");
  if (!pk.subarray(2, 10).equals(sig.subarray(2, 10))) {
    throw new Error(`signed by key ${keyId(sig.subarray(2, 10))}, but the updater trusts key ${keyId(pk.subarray(2, 10))}`);
  }
  const key = createPublicKey({ key: Buffer.concat([ED25519_SPKI_PREFIX, pk.subarray(10)]), format: "der", type: "spki" });
  const algorithm = sig.subarray(0, 2).toString("latin1");
  const message = algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
  if (!verify(null, message, key, sig.subarray(10))) throw new Error("signature does not match the file");
  const trusted = trustedLine.replace(/^trusted comment: /, "");
  if (!verify(null, Buffer.concat([sig.subarray(10), Buffer.from(trusted)]), key, Buffer.from(globalLine, "base64"))) {
    throw new Error("trusted comment signature does not match");
  }
  return Object.fromEntries(trusted.split("\t").map((field) => [field.slice(0, field.indexOf(":")), field.slice(field.indexOf(":") + 1)]));
}

const SECTION = /^##(?!#)\s*(.*?)\s*$/;
const BULLET = /^[*-] +(.+?)\s*$/;
const PULL = /^(.+) by @([A-Za-z0-9-]+(?:\[bot\])?) in https:\/\/github\.com\/[^/\s]+\/[^/\s]+\/pull\/(\d+)$/;
const GENERATED = "What's Changed";
const MAX_CHANGES = 30;

function parseNotes(body) {
  const lines = body.split(/\r?\n/);
  const generated = lines.some((line) => SECTION.exec(line)?.[1] === GENERATED);
  let section = null;
  const items = [];
  for (const line of lines) {
    const heading = SECTION.exec(line)?.[1];
    if (heading !== undefined) section = heading;
    if (line.startsWith("**Full Changelog**")) section = null;
    const text = BULLET.exec(line)?.[1];
    if (!text || (generated ? section !== GENERATED : section === "New Contributors")) continue;
    const pull = PULL.exec(text);
    items.push(pull ? { title: pull[1], author: pull[2], pr: Number(pull[3]) } : { title: text, author: null, pr: null });
  }
  return items;
}

export function changeLog(version, notes, history) {
  const current = parseVersion(version);
  if (!current) throw new Error(`${version} is not a release version`);
  const earlier = history
    .filter((release) => !release.version.nightly === !current.nightly && compareVersions(release.version, current) < 0)
    .sort((a, b) => compareVersions(b.version, a.version))
    .slice(0, MAX_CHANGES - 1);
  const entry = (text, body) => ({ version: text, items: parseNotes(body) });
  return [entry(version, notes), ...earlier.map((release) => entry(release.version.text, release.body))];
}

const sha256 = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;

export function buildManifest({ repository, tag, version, notes, history, pubDate, publicKey, files, assets }) {
  const downloadUrl = (name) => `https://github.com/${repository}/releases/download/${tag}/${encodeURIComponent(name)}`;
  const platforms = {};
  for (const bundle of bundles) {
    const name = bundle.asset(version);
    const bytes = files[name];
    if (!bytes) throw new Error(`${name} was not built`);
    const asset = assets.find((candidate) => candidate.name === name);
    if (!asset) throw new Error(`${name} is not an asset of ${tag}`);
    if (asset.state !== "uploaded" || asset.size !== bytes.length) {
      throw new Error(`${name} is not fully uploaded (state ${asset.state}, size ${asset.size} of ${bytes.length})`);
    }
    if (asset.digest && asset.digest !== sha256(bytes)) throw new Error(`${name} on ${tag} differs from the built file`);
    if (bundle.platforms.length === 0) continue;
    const signature = files[`${name}.sig`]?.toString("utf8").trim();
    if (!signature) throw new Error(`${name} has no ${name}.sig`);
    let trusted;
    try {
      trusted = verifySignature(publicKey, signature, bytes);
    } catch (error) {
      throw new Error(`${name}: ${error.message}`);
    }
    if (trusted.version !== version) {
      throw new Error(`${name}: signature binds ${trusted.version ? `version ${trusted.version}` : "no app version"}, not ${version}`);
    }
    for (const key of bundle.platforms) platforms[key] = { signature, url: downloadUrl(name) };
  }
  return { version, notes, changes: changeLog(version, notes, history), pub_date: pubDate, platforms };
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new Error(`${path}: ${error.message}`);
  }
}

function stage([os, version, outDir]) {
  const artifactPaths = JSON.parse(process.env.ARTIFACT_PATHS ?? "null");
  if (!Array.isArray(artifactPaths)) throw new Error("ARTIFACT_PATHS must be the JSON array tauri-action outputs.");
  mkdirSync(outDir, { recursive: true });
  for (const { from, to } of stagePlan(os, version, artifactPaths)) {
    if (!existsSync(from)) throw new Error(`${from} does not exist`);
    copyFileSync(from, join(outDir, to));
    console.log(`${basename(from)} -> ${to} (${statSync(from).size} bytes)`);
  }
}

function manifest([dir, tag, version, release, releases, out]) {
  const { GITHUB_REPOSITORY: repository, UPDATER_PUBKEY: publicKey } = process.env;
  if (!repository) throw new Error("GITHUB_REPOSITORY must name the repository, such as ImNallen/L1Data");
  if (!publicKey) throw new Error("UPDATER_PUBKEY must hold the updater public key");
  const files = Object.fromEntries(readdirSync(dir).map((name) => [name, readFileSync(join(dir, name))]));
  const dryRun = release === "--dry-run";
  const { body, assets } = dryRun
    ? { body: "", assets: Object.entries(files).map(([name, bytes]) => ({ name, state: "uploaded", size: bytes.length, digest: sha256(bytes) })) }
    : readJson(release);
  if (!Array.isArray(assets)) throw new Error(`${release} has no assets array`);
  const pages = readJson(releases);
  if (!Array.isArray(pages)) throw new Error(`${releases} is not an array of releases`);
  const history = parseReleases(pages.flat());
  const result = buildManifest({ repository, tag, version, notes: body ?? "", history, pubDate: new Date().toISOString(), publicKey, files, assets });
  writeFileSync(out, `${JSON.stringify(result, null, 2)}\n`);
  const urls = new Set(Object.values(result.platforms).map((entry) => entry.url));
  console.log(`latest.json for ${tag}: ${Object.keys(result.platforms).length} platforms point at ${urls.size} verified downloads.`);
}

const commands = { stage: [stage, 3], manifest: [manifest, 6] };

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const [name, ...args] = process.argv.slice(2);
    const [command, arity] = commands[name] ?? [];
    if (!command || args.length !== arity) {
      throw new Error(
        [
          "usage: node scripts/updater-manifest.mjs stage <os> <version> <out-dir>",
          "       node scripts/updater-manifest.mjs manifest <assets-dir> <tag> <version> <release.json|--dry-run> <releases.json> <out>",
        ].join("\n"),
      );
    }
    command(args);
  } catch (cause) {
    console.error(cause instanceof Error ? cause.message : cause);
    process.exitCode = 1;
  }
}

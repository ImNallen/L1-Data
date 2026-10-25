import { execFileSync } from "node:child_process";
import { appendFileSync, readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const NIGHTLY_GAP_MS = 6 * 60 * 60 * 1000;
const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-nightly\.(\d{8})\.([1-9]\d*))?$/;

export function parseVersion(text) {
  const match = VERSION.exec(text);
  if (!match) return null;
  const [, major, minor, patch, date, run] = match;
  return {
    text,
    core: [Number(major), Number(minor), Number(patch)],
    nightly: date ? { date: Number(date), run: Number(run) } : null,
  };
}

export function compareVersions(a, b) {
  for (let i = 0; i < 3; i++) if (a.core[i] !== b.core[i]) return a.core[i] - b.core[i];
  if (!a.nightly || !b.nightly) return (a.nightly ? -1 : 0) - (b.nightly ? -1 : 0);
  return a.nightly.date - b.nightly.date || a.nightly.run - b.nightly.run;
}

export function parseReleases(raw) {
  return raw.flatMap((release) => {
    const version = release.tag_name.startsWith("v") ? parseVersion(release.tag_name.slice(1)) : null;
    if (!version || release.draft || !release.published_at) return [];
    return [{ tag: release.tag_name, version, publishedAt: new Date(release.published_at) }];
  });
}

function latestStable(releases) {
  return releases.filter((r) => !r.version.nightly).sort((a, b) => compareVersions(b.version, a.version))[0] ?? null;
}

function latestNightly(releases) {
  return releases.filter((r) => r.version.nightly).sort((a, b) => b.publishedAt - a.publishedAt)[0] ?? null;
}

function nightlyRelease(facts, ci) {
  const stable = latestStable(facts.releases);
  const [major, minor, patch] = (stable?.version ?? parseVersion(facts.checkedInVersion)).core;
  const date = facts.now.toISOString().slice(0, 10).replaceAll("-", "");
  const version = `${major}.${minor}.${patch + 1}-nightly.${date}.${facts.runNumber}`;
  const previous = latestNightly(facts.releases);
  return { channel: "nightly", version, tag: `v${version}`, sha: facts.sha, previousTag: previous?.tag ?? "", ci };
}

function stableRelease(facts, version, sha, ci) {
  const parsed = parseVersion(version);
  if (!parsed || parsed.nightly) throw new Error(`A stable version is plain X.Y.Z, not ${version}.`);
  const stable = latestStable(facts.releases);
  if (stable && compareVersions(parsed, stable.version) <= 0) {
    throw new Error(`Stable ${version} is not newer than the latest stable ${stable.tag}.`);
  }
  return { channel: "stable", version, tag: `v${version}`, sha, previousTag: stable?.tag ?? "", ci };
}

function checkTagIsFree(release, facts, git) {
  if (facts.releases.some((r) => r.tag === release.tag)) throw new Error(`Release ${release.tag} is already published.`);
  const tagged = git.commitOf(release.tag);
  if (tagged && tagged !== release.sha) {
    throw new Error(`Tag ${release.tag} already points at ${tagged}, not ${release.sha}.`);
  }
}

function nightlyIsDue(facts, git) {
  const last = latestNightly(facts.releases);
  if (!last) return { due: true, reason: "No nightly has been published yet." };
  const hours = Math.floor((facts.now - last.publishedAt) / 360_000) / 10;
  if (facts.now - last.publishedAt < NIGHTLY_GAP_MS) {
    return { due: false, reason: `${last.tag} was published ${hours.toFixed(1)} h ago, less than 6 h.` };
  }
  const shipped = git.commitOf(last.tag);
  if (!shipped) throw new Error(`Tag ${last.tag} is missing from the checkout.`);
  if (shipped === facts.sha || !git.isAncestor(shipped, facts.sha)) {
    return { due: false, reason: `main has no commits since ${last.tag}.` };
  }
  return { due: true, reason: `main has new commits since ${last.tag}, published ${hours.toFixed(1)} h ago.` };
}

function promoteLatestNightly(facts, git) {
  const nightly = latestNightly(facts.releases);
  if (!nightly) throw new Error("No published nightly to promote. A stable release ships the latest nightly commit.");
  const sha = git.commitOf(nightly.tag);
  if (!sha) throw new Error(`Tag ${nightly.tag} is missing from the checkout.`);
  if (!git.isAncestor(sha, facts.sha)) throw new Error(`${nightly.tag} built ${sha}, which is not on main.`);
  const version = facts.version?.replace(/^v/, "") || nightly.version.core.join(".");
  return { release: stableRelease(facts, version, sha, false), reason: `Promote ${nightly.tag} (${sha}).` };
}

function choose(facts, git) {
  switch (facts.event) {
    case "pull_request":
      return { action: "dry-run", release: nightlyRelease(facts, false), reason: "Pull request dry run. Nothing is published." };
    case "schedule": {
      const { due, reason } = nightlyIsDue(facts, git);
      return due ? { action: "publish", release: nightlyRelease(facts, true), reason } : { action: "skip", reason };
    }
    case "workflow_dispatch": {
      if (facts.ref !== "refs/heads/main") throw new Error(`Stable and nightly releases run only from main, not ${facts.ref}.`);
      if (facts.channel === "nightly") {
        return { action: "publish", release: nightlyRelease(facts, true), reason: "Manual nightly." };
      }
      if (facts.channel === "stable") return { action: "publish", ...promoteLatestNightly(facts, git) };
      throw new Error(`Unknown channel ${facts.channel}.`);
    }
    case "push": {
      const version = facts.ref.replace(/^refs\/tags\/v/, "");
      if (version === facts.ref) throw new Error(`A release push must be a v* tag, not ${facts.ref}.`);
      return { action: "publish", release: stableRelease(facts, version, facts.sha, true), reason: `Hand-picked tag ${facts.ref}.` };
    }
    default:
      throw new Error(`Release does not handle the ${facts.event} event.`);
  }
}

export function planRelease(facts, git) {
  const plan = choose(facts, git);
  if (plan.release) checkTagIsFree(plan.release, facts, git);
  return plan;
}

export function formatOutputs(plan) {
  const r = plan.release ?? {};
  const fields = {
    action: plan.action,
    channel: r.channel ?? "",
    version: r.version ?? "",
    tag: r.tag ?? "",
    sha: r.sha ?? "",
    previous_tag: r.previousTag ?? "",
    ci: String(r.ci ?? false),
  };
  return Object.entries(fields)
    .map(([key, value]) => `${key}=${value}\n`)
    .join("");
}

const git = {
  commitOf(tag) {
    try {
      return execFileSync("git", ["rev-parse", "--verify", "--quiet", `refs/tags/${tag}^{commit}`], { encoding: "utf8" }).trim();
    } catch {
      return null;
    }
  },
  isAncestor(ancestor, descendant) {
    try {
      execFileSync("git", ["merge-base", "--is-ancestor", ancestor, descendant]);
      return true;
    } catch (error) {
      if (error.status === 1) return false;
      throw error;
    }
  },
};

function main() {
  const env = process.env;
  for (const name of ["GITHUB_REPOSITORY", "GITHUB_EVENT_NAME", "GITHUB_REF", "GITHUB_RUN_NUMBER", "RELEASE_SHA", "GITHUB_OUTPUT"]) {
    if (!env[name]) throw new Error(`${name} must be set.`);
  }
  const raw = execFileSync("gh", ["api", "--paginate", "--slurp", `repos/${env.GITHUB_REPOSITORY}/releases?per_page=100`], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  const facts = {
    event: env.GITHUB_EVENT_NAME,
    ref: env.GITHUB_REF,
    sha: env.RELEASE_SHA,
    channel: env.INPUT_CHANNEL,
    version: env.INPUT_VERSION,
    runNumber: Number(env.GITHUB_RUN_NUMBER),
    now: new Date(),
    checkedInVersion: JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version,
    releases: parseReleases(JSON.parse(raw).flat()),
  };
  const plan = planRelease(facts, git);
  const outputs = formatOutputs(plan);
  appendFileSync(env.GITHUB_OUTPUT, outputs);
  process.stdout.write(`${plan.reason}\n${outputs}`);
  if (env.GITHUB_STEP_SUMMARY) {
    const r = plan.release;
    const line = r ? `**${plan.action}** ${r.channel} \`${r.tag}\` from \`${r.sha}\`. ${plan.reason}` : `**skip**. ${plan.reason}`;
    appendFileSync(env.GITHUB_STEP_SUMMARY, `${line}\n`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (cause) {
    console.error(cause instanceof Error ? cause.message : cause);
    process.exitCode = 1;
  }
}

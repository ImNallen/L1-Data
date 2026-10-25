import assert from "node:assert/strict";
import test from "node:test";
import { compareVersions, formatOutputs, parseReleases, parseVersion, planRelease } from "./release-plan.mjs";

const main = ["c1", "c2", "c3", "c4"];
const tags = { "v0.1.4": "c1", "v0.1.5-nightly.20261020.7": "c2", "v0.1.5-nightly.20261022.9": "c3" };

const git = (overrides = {}) => ({
  commitOf: (tag) => ({ ...tags, ...overrides })[tag] ?? null,
  isAncestor: (a, b) => main.includes(a) && main.includes(b) && main.indexOf(a) <= main.indexOf(b),
});

const release = (tag, publishedAt, extra = {}) => ({ tag_name: tag, draft: false, prerelease: tag.includes("-"), published_at: publishedAt, ...extra });

const history = () => [
  release("nightly", "2026-10-22T09:00:00Z"),
  release("v0.1.5-nightly.20261022.9", "2026-10-22T09:00:00Z"),
  release("v0.1.5-nightly.20261020.7", "2026-10-20T09:00:00Z"),
  release("v0.1.4", "2026-10-19T09:00:00Z"),
  release("v0.1.9", null, { draft: true }),
];

const facts = (overrides = {}) => ({
  event: "schedule",
  ref: "refs/heads/main",
  sha: "c4",
  channel: "",
  version: "",
  runNumber: 42,
  now: new Date("2026-10-23T08:30:00Z"),
  checkedInVersion: "0.1.0",
  releases: parseReleases(history()),
  ...overrides,
});

test("a scheduled run publishes a nightly one patch above the latest stable", () => {
  assert.deepEqual(planRelease(facts(), git()), {
    action: "publish",
    reason: "main has new commits since v0.1.5-nightly.20261022.9, published 23.5 h ago.",
    release: {
      channel: "nightly",
      version: "0.1.5-nightly.20261023.42",
      tag: "v0.1.5-nightly.20261023.42",
      sha: "c4",
      previousTag: "v0.1.5-nightly.20261022.9",
      ci: true,
    },
  });
});

test("the first nightly builds on the checked-in version", () => {
  const plan = planRelease(facts({ releases: [] }), git());
  assert.equal(plan.release.version, "0.1.1-nightly.20261023.42");
  assert.equal(plan.release.previousTag, "");
});

test("a scheduled run skips when main has no new commits or the last nightly is recent", () => {
  assert.equal(planRelease(facts({ sha: "c3" }), git()).action, "skip");
  const recent = facts({ now: new Date("2026-10-22T14:59:00Z") });
  assert.deepEqual(planRelease(recent, git()), {
    action: "skip",
    reason: "v0.1.5-nightly.20261022.9 was published 5.9 h ago, less than 6 h.",
  });
  assert.equal(planRelease(facts({ now: new Date("2026-10-22T15:00:00Z") }), git()).action, "publish");
});

test("a manual nightly ignores the schedule gate but only runs from main", () => {
  const manual = facts({ event: "workflow_dispatch", channel: "nightly", sha: "c3", now: new Date("2026-10-22T10:00:00Z") });
  assert.equal(planRelease(manual, git()).action, "publish");
  assert.throws(() => planRelease({ ...manual, ref: "refs/heads/feature" }, git()), {
    message: "Stable and nightly releases run only from main, not refs/heads/feature.",
  });
});

test("a manual stable promotes the latest nightly commit without rerunning CI", () => {
  const stable = facts({ event: "workflow_dispatch", channel: "stable" });
  assert.deepEqual(planRelease(stable, git()), {
    action: "publish",
    reason: "Promote v0.1.5-nightly.20261022.9 (c3).",
    release: { channel: "stable", version: "0.1.5", tag: "v0.1.5", sha: "c3", previousTag: "v0.1.4", ci: false },
  });
  assert.equal(planRelease({ ...stable, version: "0.2.0" }, git()).release.tag, "v0.2.0");
});

test("a stable promotion refuses versions that would not move the stable feed forward", () => {
  const stable = facts({ event: "workflow_dispatch", channel: "stable" });
  assert.throws(() => planRelease({ ...stable, version: "0.1.4" }, git()), {
    message: "Stable 0.1.4 is not newer than the latest stable v0.1.4.",
  });
  assert.throws(() => planRelease({ ...stable, version: "0.2.0-nightly.20261023.1" }, git()), {
    message: "A stable version is plain X.Y.Z, not 0.2.0-nightly.20261023.1.",
  });
  const promoted = [release("v0.1.5", "2026-10-22T12:00:00Z"), ...history()];
  assert.throws(() => planRelease({ ...stable, releases: parseReleases(promoted) }, git()), {
    message: "Stable 0.1.5 is not newer than the latest stable v0.1.5.",
  });
});

test("a manual stable needs a nightly whose commit is on main", () => {
  const stable = facts({ event: "workflow_dispatch", channel: "stable" });
  assert.throws(() => planRelease({ ...stable, releases: parseReleases([release("v0.1.4", "2026-10-19T09:00:00Z")]) }, git()), {
    message: "No published nightly to promote. A stable release ships the latest nightly commit.",
  });
  assert.throws(() => planRelease(stable, git({ "v0.1.5-nightly.20261022.9": "side" })), {
    message: "v0.1.5-nightly.20261022.9 built side, which is not on main.",
  });
});

test("a pushed tag releases that commit as stable after CI", () => {
  const pushed = facts({ event: "push", ref: "refs/tags/v0.1.6", sha: "hotfix" });
  assert.deepEqual(planRelease(pushed, git({ "v0.1.6": "hotfix" })).release, {
    channel: "stable",
    version: "0.1.6",
    tag: "v0.1.6",
    sha: "hotfix",
    previousTag: "v0.1.4",
    ci: true,
  });
  assert.throws(() => planRelease({ ...pushed, ref: "refs/tags/v0.1.6-rc.1" }, git()), {
    message: "A stable version is plain X.Y.Z, not 0.1.6-rc.1.",
  });
});

test("a pull request plans a nightly-shaped dry run of its head", () => {
  const plan = planRelease(facts({ event: "pull_request", ref: "refs/pull/7/merge", sha: "head" }), git());
  assert.equal(plan.action, "dry-run");
  assert.deepEqual(plan.release, {
    channel: "nightly",
    version: "0.1.5-nightly.20261023.42",
    tag: "v0.1.5-nightly.20261023.42",
    sha: "head",
    previousTag: "v0.1.5-nightly.20261022.9",
    ci: false,
  });
});

test("a tag that is already released or points elsewhere is refused", () => {
  const published = [release("v0.1.5-nightly.20261023.42", "2026-10-23T08:00:00Z"), ...history()];
  const rerun = facts({ event: "workflow_dispatch", channel: "nightly", releases: parseReleases(published) });
  assert.throws(() => planRelease(rerun, git({ "v0.1.5-nightly.20261023.42": "c4" })), {
    message: "Release v0.1.5-nightly.20261023.42 is already published.",
  });
  const stable = facts({ event: "workflow_dispatch", channel: "stable" });
  assert.throws(() => planRelease(stable, git({ "v0.1.5": "c2" })), { message: "Tag v0.1.5 already points at c2, not c3." });
});

test("versions order nightlies below the stable they lead to", () => {
  const sorted = ["0.1.5", "0.1.5-nightly.20261022.9", "0.1.4", "0.1.5-nightly.20261022.10", "0.2.0-nightly.20261001.1"]
    .map(parseVersion)
    .sort(compareVersions)
    .map((v) => v.text);
  assert.deepEqual(sorted, ["0.1.4", "0.1.5-nightly.20261022.9", "0.1.5-nightly.20261022.10", "0.1.5", "0.2.0-nightly.20261001.1"]);
  assert.equal(parseVersion("0.1.5-nightly.2026102.1"), null);
});

test("outputs are the key=value lines later jobs read", () => {
  const plan = planRelease(facts(), git());
  assert.equal(
    formatOutputs(plan),
    [
      "action=publish",
      "channel=nightly",
      "version=0.1.5-nightly.20261023.42",
      "tag=v0.1.5-nightly.20261023.42",
      "sha=c4",
      "previous_tag=v0.1.5-nightly.20261022.9",
      "ci=true",
      "",
    ].join("\n"),
  );
  assert.equal(formatOutputs({ action: "skip", reason: "x" }), "action=skip\nchannel=\nversion=\ntag=\nsha=\nprevious_tag=\nci=false\n");
});

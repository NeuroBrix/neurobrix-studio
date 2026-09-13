import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { requiresNativeChecks, selectNativeChecks } from "./native-changes.mjs";

function temporaryDirectory(t) {
  const directory = mkdtempSync(join(tmpdir(), "studio-ci-selection-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  return directory;
}

function repository(t) {
  const directory = temporaryDirectory(t);
  const hooks = join(directory, "empty-hooks");
  mkdirSync(hooks);
  const git = (...args) =>
    execFileSync("git", ["-c", `core.hooksPath=${hooks}`, ...args], {
      cwd: directory,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
  const write = (path, content) => {
    const destination = join(directory, path);
    mkdirSync(dirname(destination), { recursive: true });
    writeFileSync(destination, content);
  };
  const commit = (message) => {
    git("add", "--all");
    git("commit", "--quiet", "--no-gpg-sign", "-m", message);
    return git("rev-parse", "HEAD");
  };

  git("init", "--quiet", "--initial-branch=main");
  git("config", "user.name", "Studio CI tests");
  git("config", "user.email", "ci-tests@example.invalid");
  write("README.md", "Studio\n");
  write("src-tauri/src/lib.rs", "pub fn available() {}\n");
  const initial = commit("Initial fixture");
  return { directory, git, write, commit, initial };
}

test("documentation-only paths and an empty diff skip native checks", () => {
  assert.equal(requiresNativeChecks([]), false);
  assert.equal(
    requiresNativeChecks([
      "README.md",
      "CONVENTIONS.md",
      "docs/setup.md",
      "docs/platforms/macos.md",
      "LICENSE",
      ".github/ISSUE_TEMPLATE/increment.md",
      ".github/ISSUE_TEMPLATE/config.yml",
      ".github/pull_request_template.md",
    ]),
    false,
  );
});

test("native, embedded frontend, configuration and script changes require checks", () => {
  for (const path of [
    "src-tauri/src/lib.rs",
    "src-tauri/Cargo.lock",
    "src-tauri/tauri.conf.json",
    "src/routes/+layout.svelte",
    "static/favicon.png",
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    ".npmrc",
    "vite.config.js",
    "tsconfig.json",
    ".cargo/config.toml",
    "rust-toolchain.toml",
    ".github/workflows/ci.yml",
    "scripts/ci/native-changes.mjs",
  ]) {
    assert.equal(requiresNativeChecks([path]), true, path);
  }
  assert.equal(requiresNativeChecks(["README.md", "src-tauri/src/lib.rs"]), true);
});

test("unknown paths do not silently skip checks", () => {
  for (const path of ["new-build-input", "other/README.md", "docs-backup/config.json"]) {
    assert.equal(requiresNativeChecks([path]), true, path);
  }
});

test("manual dispatch and a new branch push require checks without a comparison", async (t) => {
  const directory = temporaryDirectory(t);
  assert.equal(await selectNativeChecks("workflow_dispatch", {}, directory), true);
  assert.equal(
    await selectNativeChecks("push", { before: "0".repeat(40), after: "a".repeat(40) }, directory),
    true,
  );
});

test("push selection compares the full before-to-after range", async (t) => {
  const repo = repository(t);
  repo.write("docs/setup.md", "Setup notes\n");
  const docs = repo.commit("Document setup");
  assert.equal(
    await selectNativeChecks("push", { before: repo.initial, after: docs }, repo.directory),
    false,
  );
  repo.write("src-tauri/src/lib.rs", "pub fn available() -> bool { true }\n");
  const native = repo.commit("Change native fixture");
  repo.write("README.md", "Updated Studio notes\n");
  const latest = repo.commit("Update documentation after native change");
  assert.equal(
    await selectNativeChecks("push", { before: docs, after: latest }, repo.directory),
    true,
  );
  assert.equal(
    await selectNativeChecks("push", { before: native, after: latest }, repo.directory),
    false,
  );
  assert.equal(
    await selectNativeChecks("push", { before: latest, after: latest }, repo.directory),
    false,
  );
});

test("a docs-only PR stays docs-only when its base advances with a native change", async (t) => {
  const repo = repository(t);
  repo.git("switch", "--quiet", "-c", "docs-update");
  repo.write("docs/setup.md", "Setup notes\n");
  const head = repo.commit("Document setup on PR");
  repo.git("switch", "--quiet", "main");
  repo.write("src-tauri/src/lib.rs", "pub fn available() -> bool { true }\n");
  const base = repo.commit("Independent native change on main");

  assert.equal(
    await selectNativeChecks(
      "pull_request",
      { pull_request: { base: { sha: base }, head: { sha: head } } },
      repo.directory,
    ),
    false,
  );
});

test("renaming native source into documentation still requires native checks", async (t) => {
  const repo = repository(t);
  mkdirSync(join(repo.directory, "docs"));
  repo.git("mv", "src-tauri/src/lib.rs", "docs/native-notes.md");
  const after = repo.commit("Move native fixture into documentation");
  assert.equal(
    await selectNativeChecks("push", { before: repo.initial, after }, repo.directory),
    true,
  );
});

test("deleted native source still requires native checks", async (t) => {
  const repo = repository(t);
  repo.git("rm", "src-tauri/src/lib.rs");
  const after = repo.commit("Remove native fixture");
  assert.equal(
    await selectNativeChecks("push", { before: repo.initial, after }, repo.directory),
    true,
  );
});

test("missing or invalid event commit fields fail instead of skipping", async (t) => {
  const directory = temporaryDirectory(t);
  for (const [name, event] of [
    ["push", {}],
    ["push", { before: "a".repeat(40) }],
    ["push", { before: "invalid", after: "a".repeat(40) }],
    ["push", { before: "a".repeat(40), after: "b".repeat(65) }],
    ["pull_request", {}],
    ["pull_request", { pull_request: { base: { sha: "a".repeat(40) } } }],
    ["pull_request", { pull_request: { base: { sha: "a".repeat(40) }, head: { sha: "--all" } } }],
  ]) {
    await assert.rejects(async () => selectNativeChecks(name, event, directory));
  }
});

test("Git comparison failures fail instead of skipping", async (t) => {
  const directory = temporaryDirectory(t);
  await assert.rejects(async () =>
    selectNativeChecks("push", { before: "a".repeat(40), after: "b".repeat(40) }, directory),
  );

  const repo = repository(t);
  await assert.rejects(async () =>
    selectNativeChecks("push", { before: repo.initial, after: "f".repeat(40) }, repo.directory),
  );
});

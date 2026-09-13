import { execFileSync } from "node:child_process";
import { appendFileSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function requiresNativeChecks(paths) {
  return paths.some(
    (path) =>
      !(
        /^[^/]+\.md$/i.test(path) ||
        path.startsWith("docs/") ||
        path === "LICENSE" ||
        path.startsWith(".github/ISSUE_TEMPLATE/") ||
        path === ".github/pull_request_template.md"
      ),
  );
}

function requireSha(value) {
  if (typeof value !== "string" || !/^[a-f0-9]{40,64}$/i.test(value)) {
    throw new Error("The workflow event does not contain a valid commit SHA.");
  }
  return value;
}

export function selectNativeChecks(eventName, event, cwd = process.cwd()) {
  if (eventName === "workflow_dispatch") return true;

  let comparison;
  if (eventName === "pull_request") {
    const base = requireSha(event?.pull_request?.base?.sha);
    const head = requireSha(event?.pull_request?.head?.sha);
    comparison = `${base}...${head}`;
  } else if (eventName === "push") {
    const before = requireSha(event?.before);
    const after = requireSha(event?.after);
    if (/^0+$/.test(before)) return true;
    comparison = `${before}..${after}`;
  } else {
    throw new Error(`Unsupported workflow event: ${eventName}`);
  }

  // NUL separation preserves unusual filenames; disabling rename detection retains
  // both sides when a native input is renamed to a documentation path.
  const paths = execFileSync(
    "git",
    ["diff", "--name-only", "-z", "--no-renames", comparison, "--"],
    { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
  )
    .split("\0")
    .filter(Boolean);
  return requiresNativeChecks(paths);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const eventPath = process.env.GITHUB_EVENT_PATH;
  const outputPath = process.env.GITHUB_OUTPUT;
  if (!eventPath || !outputPath) throw new Error("GitHub workflow event/output paths are missing.");
  const event = JSON.parse(readFileSync(eventPath, "utf8"));
  const native = selectNativeChecks(process.env.GITHUB_EVENT_NAME, event);
  appendFileSync(outputPath, `native=${native}\n`);
  console.log(
    native ? "Native checks required." : "No native inputs changed; skipping native jobs.",
  );
}

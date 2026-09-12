import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import "@wdio/types";
import "@wdio/tauri-service";

const root = fileURLToPath(new URL("../../", import.meta.url));
const metadata = JSON.parse(
  execFileSync(
    "cargo",
    ["metadata", "--no-deps", "--format-version", "1", "--manifest-path", "src-tauri/Cargo.toml"],
    { cwd: root, encoding: "utf8" },
  ),
) as { packages: { manifest_path: string; targets: { name: string; kind: string[] }[] }[] };
const app = metadata.packages.find(
  (pkg) => pkg.manifest_path === resolve(root, "src-tauri/Cargo.toml"),
);
const binary = app?.targets.find((target) => target.kind.includes("bin"));
if (!binary) throw new Error("Cannot find the Tauri binary in Cargo metadata.");
const application = resolve(
  root,
  "src-tauri/target/e2e/debug",
  `${binary.name}${process.platform === "win32" ? ".exe" : ""}`,
);

const capabilities = [{ browserName: "tauri", "tauri:options": { application } }];

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: [fileURLToPath(new URL("./*.spec.ts", import.meta.url))],
  maxInstances: 1,
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: application,
        driverProvider: "embedded",
        captureBackendLogs: false,
        captureFrontendLogs: false,
      },
    ],
  ],
  capabilities,
  framework: "mocha",
  reporters: ["spec"],
  logLevel: "warn",
  outputDir: resolve(root, "test-results/desktop"),
  waitforTimeout: 10000,
  connectionRetryCount: 0,
  specFileRetries: 0,
  mochaOpts: { timeout: 30000 },
};

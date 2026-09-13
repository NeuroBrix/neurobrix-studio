import appConfig from "../../src-tauri/tauri.conf.json";

export const appName = appConfig.productName;

export const destinations = [
  { href: "/workspace", label: "Workspace" },
  { href: "/models", label: "Models" },
  { href: "/history", label: "History" },
  { href: "/settings", label: "Settings" },
] as const;

import { test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const tauriConf = JSON.parse(
  readFileSync(resolve(__dirname, "../../../src-tauri/tauri.conf.json"), "utf-8"),
);
const csp: string = tauriConf.app.security.csp;

test("CSP includes connect-src with ipc: and http://ipc.localhost for Tauri v2 IPC", () => {
  // Tauri v2 の custom protocol IPC は fetch('http://ipc.localhost/...') を使う。
  // connect-src にこれらが無いと postMessage フォールバックになり、
  // ipc::Response（バイナリ）が ArrayBuffer として届かない。
  expect(csp).toMatch(/connect-src[^;]*\bipc:/);
  expect(csp).toMatch(/connect-src[^;]*http:\/\/ipc\.localhost/);
});

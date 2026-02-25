import { test as base, expect } from "@playwright/test";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { access, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { constants as fsConstants } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { download as downloadEdgeDriver } from "edgedriver";
import { Builder, By, Key, until, type WebDriver } from "selenium-webdriver";

type ConfigBackup = {
  path: string;
  existed: boolean;
  content: string;
};

type Harness = {
  driver: WebDriver;
  tauriDriver: ChildProcessWithoutNullStreams;
  backup: ConfigBackup;
};

type WindowState = {
  handle: string;
  title: string;
  visibility: string;
  label: string;
};

const WD_SERVER = "http://127.0.0.1:4444/";

const E2E_CONFIG_TOML = `
[hotkey]
modifier = "Alt"
key = "Q"

[general]
hotkey_toggle = true
show_on_startup = true
auto_hide_on_focus_lost = false
show_tray_icon = false
ime_off_on_show = false

[appearance]
max_results = 8
window_width = 600
top_n_history = 200
max_history_display = 8
show_icons = false

[visual]
preset = "obsidian"
background_color = "#282828"
input_background_color = "#383838"
text_color = "#e0e0e0"
selected_row_color = "#505050"
hint_text_color = "#808080"
font_family = "Segoe UI"
font_size = 15

[paths]
additional = []
scan = []

[search]
normal_mode = "fuzzy"
folder_mode = "fuzzy"
show_hidden_system = false
history_normalization = "disabled"
fuzzy_history_cap_ratio = 0.30
`.trim();

async function fileExists(targetPath: string): Promise<boolean> {
  try {
    await access(targetPath, fsConstants.F_OK);
    return true;
  } catch {
    return false;
  }
}

function getConfigPath(): string {
  const appData = process.env.APPDATA ?? path.join(os.homedir(), "AppData", "Roaming");
  return path.join(appData, "Snotra", "config.toml");
}

async function prepareE2EConfig(): Promise<ConfigBackup> {
  const configPath = getConfigPath();
  const existed = await fileExists(configPath);
  const content = existed ? await readFile(configPath, "utf8") : "";
  await mkdir(path.dirname(configPath), { recursive: true });
  await writeFile(configPath, `${E2E_CONFIG_TOML}\n`, "utf8");
  return { path: configPath, existed, content };
}

async function restoreConfig(backup: ConfigBackup): Promise<void> {
  if (backup.existed) {
    await writeFile(backup.path, backup.content, "utf8");
    return;
  }
  await rm(backup.path, { force: true });
}

function getAppBinaryPath(): string {
  const defaultName = process.platform === "win32" ? "snotra.exe" : "snotra";
  return process.env.SNOTRA_E2E_APP ?? path.resolve(process.cwd(), "target", "release", defaultName);
}

function getTauriDriverPath(): string {
  const binaryName = process.platform === "win32" ? "tauri-driver.exe" : "tauri-driver";
  if (process.env.TAURI_DRIVER_PATH && process.env.TAURI_DRIVER_PATH.trim() !== "") {
    return process.env.TAURI_DRIVER_PATH;
  }
  const cargoHome = process.env.CARGO_HOME ?? path.join(os.homedir(), ".cargo");
  return path.join(cargoHome, "bin", binaryName);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForPort(port: number, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const connected = await new Promise<boolean>((resolve) => {
      const socket = net.createConnection({ host: "127.0.0.1", port });
      socket.once("connect", () => {
        socket.destroy();
        resolve(true);
      });
      socket.once("error", () => resolve(false));
    });
    if (connected) return;
    await sleep(120);
  }
  throw new Error(`Timed out waiting for port ${port}`);
}

async function spawnTauriDriver(): Promise<ChildProcessWithoutNullStreams> {
  const tauriDriverPath = getTauriDriverPath();
  if (!(await fileExists(tauriDriverPath))) {
    throw new Error(
      `tauri-driver not found at ${tauriDriverPath}. Run: npm run e2e:tauri:setup`,
    );
  }
  const nativeDriverPath = await downloadEdgeDriver();
  const proc = spawn(tauriDriverPath, ["--native-driver", nativeDriverPath], {
    stdio: "pipe",
  });
  proc.on("error", () => {
    // handled by port timeout / session creation errors
  });
  await waitForPort(4444, 12_000);
  return proc;
}

async function createWebDriverSession(): Promise<WebDriver> {
  const appBinary = getAppBinaryPath();
  if (!(await fileExists(appBinary))) {
    throw new Error(
      `App binary not found at ${appBinary}. Run: npm run build && cargo build -p snotra --release`,
    );
  }

  return new Builder()
    .usingServer(WD_SERVER)
    .withCapabilities({
      browserName: "wry",
      "tauri:options": {
        application: appBinary,
      },
    })
    .build();
}

async function collectWindowStates(driver: WebDriver): Promise<WindowState[]> {
  const handles = await driver.getAllWindowHandles();
  const states: WindowState[] = [];
  for (const handle of handles) {
    await driver.switchTo().window(handle);
    const [title, visibility, label] = await Promise.all([
      driver.getTitle().catch(() => ""),
      driver
        .executeScript<string>("return document.visibilityState || 'unknown';")
        .catch(() => "unknown"),
      driver
        .executeScript<string>(
          "return window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label || '';",
        )
        .catch(() => ""),
    ]);
    states.push({ handle, title, visibility, label });
  }
  return states;
}

async function waitForVisibleLabel(
  driver: WebDriver,
  expectedLabel: string,
  timeoutMs: number,
): Promise<void> {
  await driver.wait(async () => {
    const states = await collectWindowStates(driver);
    return states.some((state) => state.label === expectedLabel && state.visibility === "visible");
  }, timeoutMs);
}

async function switchToLabel(driver: WebDriver, expectedLabel: string): Promise<boolean> {
  const states = await collectWindowStates(driver);
  const target = states.find((state) => state.label === expectedLabel);
  if (!target) {
    return false;
  }
  await driver.switchTo().window(target.handle);
  return true;
}

async function waitAndSwitchToLabel(
  driver: WebDriver,
  expectedLabel: string,
  timeoutMs: number,
): Promise<void> {
  await driver.wait(async () => switchToLabel(driver, expectedLabel), timeoutMs);
}

async function createHarness(): Promise<Harness> {
  const backup = await prepareE2EConfig();
  let tauriDriver: ChildProcessWithoutNullStreams | undefined;
  let driver: WebDriver | undefined;
  try {
    tauriDriver = await spawnTauriDriver();
    driver = await createWebDriverSession();
    await waitAndSwitchToLabel(driver, "main", 12_000);
    try {
      await driver.wait(until.elementLocated(By.css(".search-input")), 12_000);
    } catch (e) {
      const states = await collectWindowStates(driver).catch(() => []);
      const snapshots: Array<{ label: string; visibility: string; body: string }> = [];
      for (const state of states) {
        await driver.switchTo().window(state.handle);
        const body = await driver
          .executeScript<string>("return document.body?.innerText || '';")
          .catch(() => "");
        snapshots.push({
          label: state.label,
          visibility: state.visibility,
          body: body.slice(0, 180),
        });
      }
      throw new Error(
        `search-input not found. states=${JSON.stringify(states)} snapshots=${JSON.stringify(snapshots)} cause=${String(e)}`,
      );
    }
    return { driver, tauriDriver, backup };
  } catch (e) {
    if (driver) {
      await driver.quit().catch(() => {});
    }
    if (tauriDriver && !tauriDriver.killed) {
      tauriDriver.kill();
    }
    await restoreConfig(backup).catch(() => {});
    throw e;
  }
}

async function disposeHarness(harness: Harness): Promise<void> {
  await harness.driver.quit().catch(() => {});
  if (!harness.tauriDriver.killed) {
    harness.tauriDriver.kill();
  }
  await restoreConfig(harness.backup);
}

const test = base.extend<{ harness: Harness }>({
  harness: async ({}, use) => {
    const harness = await createHarness();
    try {
      await use(harness);
    } finally {
      await disposeHarness(harness);
    }
  },
});

test("startup shows main input and accepts typing", async ({ harness }) => {
  const input = await harness.driver.findElement(By.css(".search-input"));
  await input.sendKeys("abc");
  await harness.driver.wait(async () => {
    const value = await input.getAttribute("value");
    return value === "abc";
  }, 5_000);
  const value = await input.getAttribute("value");
  expect(value).toBe("abc");
});

test("slash /a switches visible window to about", async ({ harness }) => {
  const input = await harness.driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE, "/a");
  await waitForVisibleLabel(harness.driver, "about", 8_000);
  const states = await collectWindowStates(harness.driver);
  expect(states.some((state) => state.label === "about" && state.visibility === "visible")).toBe(
    true,
  );
});

test("slash /o switches visible window to settings", async ({ harness }) => {
  const input = await harness.driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE, "/o");
  await waitForVisibleLabel(harness.driver, "settings", 8_000);
  const states = await collectWindowStates(harness.driver);
  expect(
    states.some((state) => state.label === "settings" && state.visibility === "visible"),
  ).toBe(true);
});

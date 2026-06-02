import { test as base, expect } from "@playwright/test";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { access, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
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
  fixtureDir: string;
};

type WindowState = {
  handle: string;
  title: string;
  visibility: string;
  label: string;
  nativeVisible: boolean | null;
};

const WD_SERVER = "http://127.0.0.1:4444/";
const E2E_BUILD_HINT = "Run: npm run e2e:tauri:setup (or: npx tauri build --no-bundle)";

const E2E_FIXTURE_DIR = path.join(os.tmpdir(), "snotra-e2e-fixtures");
const E2E_FIXTURE_FILENAMES = ["snotra-e2e-alpha.txt", "snotra-e2e-beta.txt", "snotra-e2e-gamma.txt"];
const E2E_SEARCH_QUERY = "snotra-e2e";

function buildE2EConfigToml(fixtureDir: string): string {
  const escapedDir = fixtureDir.replace(/\\/g, "\\\\");
  return `
[hotkey]
modifier = "Alt"
key = "Q"

[general]
language = "ja"
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

[[paths.scan]]
path = "${escapedDir}"
extensions = [".txt"]
include_folders = false

[search]
normal_mode = "fuzzy"
folder_mode = "fuzzy"
show_hidden_system = false
history_normalization = "disabled"
fuzzy_history_cap_ratio = 0.30

[[openers]]
target = "folder"

[[openers.tools]]
name = "cmd"
exe = "cmd.exe"

[[openers.tools]]
name = "PowerShell"
exe = "powershell.exe"

[[openers]]
target = "ext:txt"

[[openers.tools]]
name = "Notepad"
exe = "notepad.exe"

[[openers.tools]]
name = "Type"
exe = "cmd.exe"
args = '/c type "{path}"'
`.trim();
}

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

async function prepareE2EConfig(fixtureDir: string): Promise<ConfigBackup> {
  const configPath = getConfigPath();
  const existed = await fileExists(configPath);
  const content = existed ? await readFile(configPath, "utf8") : "";
  await mkdir(path.dirname(configPath), { recursive: true });
  await writeFile(configPath, `${buildE2EConfigToml(fixtureDir)}\n`, "utf8");
  return { path: configPath, existed, content };
}

async function restoreConfig(backup: ConfigBackup): Promise<void> {
  if (backup.existed) {
    await writeFile(backup.path, backup.content, "utf8");
    return;
  }
  await rm(backup.path, { force: true });
}

async function setupFixtureDir(): Promise<string> {
  const dir = E2E_FIXTURE_DIR;
  await mkdir(dir, { recursive: true });
  for (const name of E2E_FIXTURE_FILENAMES) {
    await writeFile(path.join(dir, name), `E2E test fixture: ${name}\n`, "utf8");
  }
  return dir;
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

/**
 * msedgedriver must match the **WebView2 Runtime** the app embeds, not the Edge
 * browser. The `edgedriver` package resolves the driver from the installed Edge
 * browser version, which can differ from the WebView2 Runtime by a patch level —
 * the mismatch makes every session fail with
 * "session not created: Chrome instance exited". Detect the runtime version from
 * its install directory so the downloaded driver matches what the app actually uses.
 * Returns undefined if not found (caller falls back to the package's auto-detect).
 */
async function resolveWebView2DriverVersion(): Promise<string | undefined> {
  if (process.platform !== "win32") return undefined;
  const bases = [
    "C:\\Program Files (x86)\\Microsoft\\EdgeWebView\\Application",
    "C:\\Program Files\\Microsoft\\EdgeWebView\\Application",
  ];
  const cmpVersion = (a: string, b: string): number => {
    const pa = a.split(".").map(Number);
    const pb = b.split(".").map(Number);
    for (let i = 0; i < 4; i++) {
      if ((pa[i] ?? 0) !== (pb[i] ?? 0)) return (pa[i] ?? 0) - (pb[i] ?? 0);
    }
    return 0;
  };
  for (const base of bases) {
    try {
      const names = await readdir(base);
      const versions = names.filter((n) => /^\d+\.\d+\.\d+\.\d+$/.test(n)).sort(cmpVersion);
      if (versions.length > 0) return versions[versions.length - 1];
    } catch {
      // directory absent — try next base
    }
  }
  return undefined;
}

async function spawnTauriDriver(): Promise<ChildProcessWithoutNullStreams> {
  const tauriDriverPath = getTauriDriverPath();
  if (!(await fileExists(tauriDriverPath))) {
    throw new Error(
      `tauri-driver not found at ${tauriDriverPath}. Run: npm run e2e:tauri:setup`,
    );
  }
  // Pin msedgedriver to the WebView2 Runtime version (unless explicitly overridden
  // via EDGEDRIVER_VERSION). edgedriver otherwise matches the Edge browser, which
  // can differ from the WebView2 Runtime and break session creation.
  if (!process.env.EDGEDRIVER_VERSION) {
    const wv2Version = await resolveWebView2DriverVersion();
    if (wv2Version) {
      process.env.EDGEDRIVER_VERSION = wv2Version;
    }
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
    throw new Error(`App binary not found at ${appBinary}. ${E2E_BUILD_HINT}`);
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
    let nativeVisible: boolean | null = null;
    if (label) {
      nativeVisible = await driver
        .executeAsyncScript<boolean | null>(
          `
            const done = arguments[arguments.length - 1];
            const label = window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label;
            if (!label) {
              done(null);
              return;
            }
            window.__TAURI_INTERNALS__.invoke("plugin:window|is_visible", { label })
              .then((visible) => done(Boolean(visible)))
              .catch(() => done(null));
          `,
        )
        .catch(() => null);
    }
    states.push({ handle, title, visibility, label, nativeVisible });
  }
  return states;
}

async function getMainAlwaysOnTop(driver: WebDriver): Promise<boolean | null> {
  const switched = await switchToLabel(driver, "main");
  if (!switched) return null;
  return driver.executeAsyncScript<boolean>(`
    const done = arguments[arguments.length - 1];
    window.__TAURI_INTERNALS__.invoke('plugin:window|is_always_on_top', { label: 'main' })
      .then(done)
      .catch(() => done(null));
  `);
}

async function waitForHiddenLabel(
  driver: WebDriver,
  label: string,
  timeoutMs: number,
): Promise<void> {
  await driver.wait(async () => {
    const states = await collectWindowStates(driver);
    const target = states.find((s) => s.label === label);
    return (
      !target ||
      target.nativeVisible === false ||
      (target.nativeVisible == null && target.visibility !== "visible")
    );
  }, timeoutMs);
}

async function waitForVisibleLabel(
  driver: WebDriver,
  expectedLabel: string,
  timeoutMs: number,
): Promise<void> {
  await driver.wait(async () => {
    const states = await collectWindowStates(driver);
    return states.some(
      (state) =>
        state.label === expectedLabel &&
        (state.nativeVisible === true ||
          (state.nativeVisible == null && state.visibility === "visible")),
    );
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

function isDevUrlFallbackSnapshot(body: string): boolean {
  return body.includes("ERR_CONNECTION_REFUSED") || body.includes("接続が拒否されました");
}

// 検索クエリを「1 回だけ」入力する（#369）。再インデックス系の検証で毎ポール打ち直すと、
// 打ち直しのたびに query が空→再入力で searchGeneration を bump し続ける。CI の遅い検索 IPC では
// 検索が完了する前に次の打ち直しが generation を進めるため、refreshResults の staleness ガード
// （requestId !== searchGeneration）が全 in-flight 検索を破棄し、results() が永遠に空のままになる。
// 再インデックス完了は indexing-complete → runRefresh() がフロント側で自動再検索するため、
// 入力は 1 回で足り、以降はポールで行数だけを観測すれば再構築結果が反映される。
async function typeQueryOnce(driver: WebDriver, query: string): Promise<void> {
  await switchToLabel(driver, "main");
  const el = await driver.findElement(By.css(".search-input"));
  await el.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE, query);
}

async function createHarness(): Promise<Harness> {
  const fixtureDir = await setupFixtureDir();
  const backup = await prepareE2EConfig(fixtureDir);
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
      const buildHint = snapshots.some((snapshot) => isDevUrlFallbackSnapshot(snapshot.body))
        ? ` likely loaded devUrl instead of bundled assets. Rebuild the app binary with 'npx tauri build --no-bundle'; 'cargo build -p snotra --release' alone is not sufficient for this E2E harness.`
        : "";
      throw new Error(
        `search-input not found.${buildHint} states=${JSON.stringify(states)} snapshots=${JSON.stringify(snapshots)} cause=${String(e)}`,
      );
    }
    return { driver, tauriDriver, backup, fixtureDir };
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
  await rm(harness.fixtureDir, { recursive: true, force: true }).catch(() => {});
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

test("slash /o コマンドが実行され main の alwaysOnTop が false になる", async ({ harness }) => {
  const { driver } = harness;

  // snotra-settings は egui ネイティブウィンドウのため WebDriver から不可視
  // /o の副作用（alwaysOnTop → false）でコマンド実行を確認する
  await switchToLabel(driver, "main");
  const input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE, "/o");

  await driver.wait(async () => {
    return (await getMainAlwaysOnTop(driver)) === false;
  }, 8_000, "/o コマンド後に alwaysOnTop が false にならない");

  expect(await getMainAlwaysOnTop(driver)).toBe(false);
});


test("Shift+Enter でツール選択リストが表示され Escape で元に戻る", async ({ harness }) => {
  const { driver } = harness;

  // E2E フィクスチャのファイルを検索して結果を表示
  await switchToLabel(driver, "main");
  let input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(E2E_SEARCH_QUERY);

  // 結果が main ウィンドウ内の DOM に表示されるまで待つ
  await driver.wait(
    async () => (await driver.findElements(By.css(".result-row"))).length > 0,
    8_000,
    "検索結果が表示されない",
  );

  // Shift+Enter でツール選択へ（ext:txt openers に 2 ツール定義済み）
  input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.chord(Key.SHIFT, Key.ENTER));

  // placeholder が "ツールを選択..." に変わるまでポーリング
  await driver.wait(async () => {
    const el = await driver.findElement(By.css(".search-input"));
    return (await el.getAttribute("placeholder")) === "ツールを選択...";
  }, 6_000, "tool selection did not activate");

  // Escape でツール選択を解除
  await driver.actions().sendKeys(Key.ESCAPE).perform();

  // placeholder が "ツールを選択..." 以外に戻るまでポーリング
  await driver.wait(async () => {
    const el = await driver.findElement(By.css(".search-input"));
    return (await el.getAttribute("placeholder")) !== "ツールを選択...";
  }, 6_000, "tool selection did not exit");

  // 元の query が復元されていることを確認
  const finalEl = await driver.findElement(By.css(".search-input"));
  const value = await finalEl.getAttribute("value");
  expect(value).toBe(E2E_SEARCH_QUERY);
});


test("検索クエリを入力すると結果が表示される", async ({ harness }) => {
  const { driver } = harness;

  await switchToLabel(driver, "main");
  const input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(E2E_SEARCH_QUERY);

  // 結果が main ウィンドウ内の DOM に表示されるまで待つ
  await driver.wait(
    async () => (await driver.findElements(By.css(".result-row"))).length > 0,
    8_000,
    "result-row が表示されない",
  );
  const rows = await driver.findElements(By.css(".result-row"));
  expect(rows.length).toBeGreaterThan(0);
});

test("↓↑ キーで選択行が移動する", async ({ harness }) => {
  const { driver } = harness;

  await switchToLabel(driver, "main");
  let input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(E2E_SEARCH_QUERY);

  await driver.wait(
    async () => (await driver.findElements(By.css(".result-row"))).length > 1,
    8_000,
    "結果に2行以上表示されない",
  );

  // 初期状態: 先頭行が selected
  const rows = await driver.findElements(By.css(".result-row"));
  const firstClass = await rows[0].getAttribute("class");
  expect(firstClass).toContain("selected");

  // ↓ キーで2番目の行へ移動
  input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.ARROW_DOWN);

  await driver.wait(async () => {
    const r = await driver.findElements(By.css(".result-row"));
    if (r.length < 2) return false;
    return (await r[1].getAttribute("class"))?.includes("selected") ?? false;
  }, 4_000, "↓ キーで選択が2番目に移動しない");

  // ↑ キーで先頭に戻る
  input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.ARROW_UP);

  await driver.wait(async () => {
    const r = await driver.findElements(By.css(".result-row"));
    if (r.length === 0) return false;
    return (await r[0].getAttribute("class"))?.includes("selected") ?? false;
  }, 4_000, "↑ キーで選択が先頭に戻らない");
});

test("Escape で main ウィンドウが非表示になる", async ({ harness }) => {
  const { driver } = harness;

  await waitForVisibleLabel(driver, "main", 4_000);
  await switchToLabel(driver, "main");
  const input = await driver.findElement(By.css(".search-input"));
  // 空クエリの状態で Escape → main が非表示になる
  await input.sendKeys(Key.ESCAPE);
  await waitForHiddenLabel(driver, "main", 6_000);
});

test("/o 後に IPC チャネルが生存している", async ({ harness }) => {
  const { driver } = harness;

  // /o で snotra-settings を起動（egui ネイティブウィンドウ、WebDriver からは不可視）
  await switchToLabel(driver, "main");
  let input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE, "/o");

  // alwaysOnTop が false になることで /o の実行を確認
  await driver.wait(async () => {
    return (await getMainAlwaysOnTop(driver)) === false;
  }, 8_000, "/o コマンドが実行されない");

  // IPC 生存確認: /r コマンドが正常に処理され main が表示されたままになること
  await switchToLabel(driver, "main");
  input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE, "/r");

  await driver.wait(async () => {
    await switchToLabel(driver, "main");
    const el = await driver.findElement(By.css(".search-input"));
    return (await el.getAttribute("value")) === "/r";
  }, 4_000, "IPC 応答なし: /r コマンドが処理されない");

  await waitForVisibleLabel(driver, "main", 4_000);
});

test("← キーでフォルダ展開、Escape でスナップショット復帰", async ({ harness }) => {
  const { driver } = harness;

  // E2E フィクスチャのファイルを検索して結果を表示
  await switchToLabel(driver, "main");
  let input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(E2E_SEARCH_QUERY);

  await driver.wait(
    async () => (await driver.findElements(By.css(".result-row"))).length > 0,
    8_000,
    "検索結果が表示されない",
  );

  // ← キーで選択中ファイルの親ディレクトリを展開 → folderFilter="" で input value が空になる
  input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.ARROW_LEFT);

  await driver.wait(async () => {
    const el = await driver.findElement(By.css(".search-input"));
    return (await el.getAttribute("value")) === "";
  }, 4_000, "← キーでフォルダモードに入らない（input が空にならない）");

  // Escape でスナップショット復帰（元のクエリと結果に戻る）
  await driver.actions().sendKeys(Key.ESCAPE).perform();

  await driver.wait(async () => {
    const el = await driver.findElement(By.css(".search-input"));
    return (await el.getAttribute("value")) === E2E_SEARCH_QUERY;
  }, 4_000, `Escape で "${E2E_SEARCH_QUERY}" に復帰しない`);
});

test("/r 入力でエラーにならず main が表示されたままになる", async ({ harness }) => {
  const { driver } = harness;

  await switchToLabel(driver, "main");
  const input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys("/r");

  // 検索処理が完了するまで input value が /r になるのを待つ
  await driver.wait(async () => {
    await switchToLabel(driver, "main");
    const el = await driver.findElement(By.css(".search-input"));
    return (await el.getAttribute("value")) === "/r";
  }, 4_000);

  // main ウィンドウが表示されたまま（クラッシュ・自動非表示なし）
  await waitForVisibleLabel(driver, "main", 4_000);
  await switchToLabel(driver, "main");
  const el = await driver.findElement(By.css(".search-input"));
  expect(await el.getAttribute("value")).toBe("/r");
});

test("/s 入力で main ウィンドウが非表示になる", async ({ harness }) => {
  const { driver } = harness;

  await switchToLabel(driver, "main");
  const input = await driver.findElement(By.css(".search-input"));
  // /s action: hideMainWindow() → rebuildIndex()（hide が先行する）
  await input.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE, "/s");

  await waitForHiddenLabel(driver, "main", 8_000);
});

test("config.toml を書き換えると max_results が即時反映される", async ({ harness }) => {
  const { driver, backup, fixtureDir } = harness;

  // ベースライン: フィクスチャファイル全件が表示される（max_results = 8）
  await switchToLabel(driver, "main");
  let input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(E2E_SEARCH_QUERY);

  await driver.wait(
    async () =>
      (await driver.findElements(By.css(".result-row"))).length === E2E_FIXTURE_FILENAMES.length,
    8_000,
    `ベースライン: ${E2E_FIXTURE_FILENAMES.length} 件が表示されない`,
  );

  // ベースラインのウィンドウ高さを取得（max_results = 8）
  const baseHeight = await driver.executeScript<number>("return window.innerHeight;");

  // config.toml を max_results = 1 に書き換えてホットリロードをトリガー
  const modifiedConfig = buildE2EConfigToml(fixtureDir).replace("max_results = 8", "max_results = 1");
  await writeFile(backup.path, `${modifiedConfig}\n`, "utf8");

  // config が反映されてウィンドウ高さが縮小するまで待つ
  // max_results = 1 → 52 + 1*30 + 8 = 90 (logical) < baseHeight
  await driver.wait(async () => {
    // 再検索をトリガーして結果を表示状態に保つ
    const el = await driver.findElement(By.css(".search-input"));
    await el.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE, E2E_SEARCH_QUERY);
    const height = await driver.executeScript<number>("return window.innerHeight;");
    return height > 0 && height < baseHeight;
  }, 12_000, "config.toml の max_results 変更が反映されない（ウィンドウ高さが縮小しない）");
});

test("/s 後にインデックス再構築が完了し検索が機能する", async ({ harness }) => {
  const { driver } = harness;

  // /s でインデックス再構築を起動 → main が非表示になる
  await switchToLabel(driver, "main");
  let input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE, "/s");
  await waitForHiddenLabel(driver, "main", 8_000);

  // Tauri IPC で main を再表示（capabilities に core:window:allow-show あり）
  await switchToLabel(driver, "main");
  await driver.executeAsyncScript(`
    const done = arguments[arguments.length - 1];
    window.__TAURI_INTERNALS__.invoke("plugin:window|show", { label: "main" })
      .then(() => done(null))
      .catch(() => done(null));
  `);
  await waitForVisibleLabel(driver, "main", 6_000);

  // クエリは 1 回だけ入力し（#369: 毎ポール打ち直しは CI で全検索を staleness 破棄させる）、
  // 行が出るまでポールする。再構築完了は indexing-complete → runRefresh() が自動再検索する。
  await typeQueryOnce(driver, E2E_SEARCH_QUERY);
  await driver.wait(async () => {
    await switchToLabel(driver, "main");
    return (await driver.findElements(By.css(".result-row"))).length > 0;
  }, 30_000, "インデックス再構築後に検索結果が表示されない");

  const rows = await driver.findElements(By.css(".result-row"));
  expect(rows.length).toBeGreaterThan(0);
});

test("include_path_env の切り替えで PATH 実行ファイルが検索結果に出入りする", async ({ harness }) => {
  const { driver, backup, fixtureDir } = harness;
  const PATH_QUERY = "cargo";

  // Phase 1: include_path_env = false（デフォルト）→ "cargo" は結果に出ない
  await switchToLabel(driver, "main");
  let input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(PATH_QUERY);
  // 検索処理が安定するまで少し待つ
  await sleep(1_500);
  let rows = await driver.findElements(By.css(".result-row"));
  expect(rows.length).toBe(0);

  // Phase 2: include_path_env = true に書き換え → config_watcher が再インデックス
  const configWithPath = buildE2EConfigToml(fixtureDir).replace(
    "show_hidden_system = false",
    "show_hidden_system = false\ninclude_path_env = true",
  );
  await writeFile(backup.path, `${configWithPath}\n`, "utf8");

  // クエリは 1 回だけ入力し（#369）、行が出るまでポール。config 変更による再インデックス完了は
  // indexing-complete → runRefresh() が自動で "cargo" を再検索するため、打ち直しは不要かつ有害。
  await typeQueryOnce(driver, PATH_QUERY);
  await driver.wait(async () => {
    await switchToLabel(driver, "main");
    return (await driver.findElements(By.css(".result-row"))).length > 0;
  }, 30_000, "include_path_env = true に切り替え後、PATH 実行ファイルが検索結果に出ない");

  // Phase 3: include_path_env = false に戻す → PATH 結果が消える
  await writeFile(backup.path, `${buildE2EConfigToml(fixtureDir)}\n`, "utf8");

  // 打ち直さない（#369: Phase 2 で "cargo" が表示中。打ち直すと一瞬の空表示で ===0 を誤判定する）。
  // config 無効化 → 再インデックス完了 → runRefresh() が "cargo" を再検索し 0 件に落ちるのをポールする。
  await driver.wait(async () => {
    await switchToLabel(driver, "main");
    return (await driver.findElements(By.css(".result-row"))).length === 0;
  }, 30_000, "include_path_env = false に切り替え後、PATH 実行ファイルが検索結果に残っている");

  rows = await driver.findElements(By.css(".result-row"));
  expect(rows.length).toBe(0);
});

test("Enter で検索結果を起動すると main が非表示になる", async ({ harness }) => {
  const { driver } = harness;

  await switchToLabel(driver, "main");
  let input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(E2E_SEARCH_QUERY);

  await driver.wait(
    async () => (await driver.findElements(By.css(".result-row"))).length > 0,
    8_000,
  );

  // Enter で先頭の結果（snotra-e2e-*.txt）を起動
  // 起動成功 → hideMain() で main が非表示になる（side effect: txt がエディタで開く）
  input = await driver.findElement(By.css(".search-input"));
  await input.sendKeys(Key.ENTER);

  await waitForHiddenLabel(driver, "main", 6_000);
});

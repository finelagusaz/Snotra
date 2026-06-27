# research.md — issue #382

## issue の要約

E2E テスト `↓↑ キーで選択行が移動する`（`e2e/tauri.slash.e2e.ts`）の **初期 selected 確認部（L545-548）** が、`StaleElementReferenceError` による非決定的 flake を起こす。`.result-row` 要素を `findElements` で掴み置きしてから `getAttribute("class")` を読む型のため、通常検索デバウンス（leading + trailing 50ms）の trailing リフレッシュが結果リストを再レンダリングすると、掴んだ `rows[0]` が DOM から外れて stale 化する。同テストの ↓/↑ 確認部は既に `driver.wait` 内 re-find 型で堅牢。**この初期確認だけを同じ型へ揃える**のが本 issue。

- コードの挙動・仕様は変えない（テスト堅牢化のみ）。**バグではなく flake 体質の除去**。

## 関連コード

- `e2e/tauri.slash.e2e.ts`
  - **L545-548**: 修正対象。`const rows = await driver.findElements(...)` → `rows[0].getAttribute("class")` の掴み置き型。
  - **L554-558 / L564-568**: 既存の堅牢な参照型（`driver.wait` 内で毎回 `findElements` し直し、`r[i].getAttribute("class")?.includes("selected")` を判定）。修正のテンプレートになる。
  - L539-543: `result-row` が 2 行以上表示されるまで待つ `driver.wait`（修正対象の直前。これ自体は `.length` 参照のみで安全）。
- `docs/build-commands.md`
  - **L71-83「E2E/スモーク運用メモ」**: E2E の落とし穴を一行ずつ記録するセクション。一般則（再レンダリング要素の属性アサーションは掴み置きせず `driver.wait` 内 re-find）の追記先。

## 既存パターン

- **`driver.wait` 内 re-find 型は同ファイル内に既存**（L554-558, L564-568）。新規パターンの導入は不要で、初期確認を既存型へ揃えるだけ。
- issue 本文に修正コードが完成形で提示されている:

  ```ts
  await driver.wait(async () => {
    const r = await driver.findElements(By.css(".result-row"));
    if (r.length === 0) return false;
    return (await r[0].getAttribute("class"))?.includes("selected") ?? false;
  }, 4_000, "初期状態で先頭行が selected にならない");
  ```

## 技術的制約

- E2E は `tauri-driver + selenium-webdriver + edgedriver`（Playwright runner 上）。WebDriver の `StaleElementReferenceError` は「掴んだ要素ハンドルが対応 DOM ノードを失った」状態。再レンダリングで必ず発生しうる。
- Win32 / IPC 境界・リアクティブ制約には触れない。純粋に E2E テストファイルとドキュメントのみの変更。
- 検証は CI の `E2E & Smoke` workflow（`e2e` ラベル付き PR / 手動 dispatch）。ローカルでも `npm run e2e:tauri` で再現確認可能だが、flake は非決定的なため「失敗の消滅」はグリーン継続で確認する性質。

## 同一パターンの全コードパス検索（根本原因スイープ）

根本原因 = 「再レンダリングされうる `.result-row` を `findElements` で掴み置きして属性を読む」。
- issue の確認どおり、E2E 内で `.result-row` を掴み置きして `getAttribute` を読むのは **L546-548 のみ**。
- 他の `.result-row` 取得（L528, L540, L546 直前の wait 等）は `.length` 参照のみで stale の影響を受けない。
- 他の `getAttribute` は概ね安定要素 `.search-input` か、既に `driver.wait` 内 re-find のため低リスク。
- → 修正対象は L545-548 の 1 箇所で過不足なし。

## 未解決の疑問

なし。issue が根本原因・修正・受け入れ条件まで一意に提示している。

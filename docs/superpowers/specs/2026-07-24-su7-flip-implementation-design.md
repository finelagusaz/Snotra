# SU7: flip 実装（既定 egui 化 + WebView2 撤去 + e2e 後継）設計

日付: 2026-07-24 / 対象: #532 Phase 2 SU7 のうち flip 実装と e2e 後継方針 / 先行: 配布・検証方針 spec（`2026-07-24-su7-distribution-verification-design.md`・PR #657 マージ済み）
ロードマップ: `2026-07-21-phase2-softbuffer-migration-roadmap.md` の SU7 行
スコープ確定: 配布 spec 決定 4（v0.18.3 = WebView2 最終版・v0.19.0 で flip + 撤去を同一リリース完結）を実装へ落とす。brainstorm で **e2e 後継方針も本 spec へ畳み込む**と決定（WebView2 撤去の確定により #567 の移行先が消え、「後継方針」の実体が「egui smoke を CI に立て #567 を閉じる」へ収束したため）。

## 背景と言語化

依存面の走査（Explore・2026-07-24）で構造の要点が 4 つ確定した。

- **フラグ判定は製品コード 7 箇所に閉じる**（`main.rs` ×5: 宣言窓 retain / single-instance / setup 内 egui 初期化 / hotkey listener / `setup_startup_display`、`commands/window.rs` ×2: alwaysOnTop の get_window 分岐）。いずれも素直な if/else で、flip は「egui 枝の無条件化 + else 削除」に帰着する。
- **egui 経路は IPC を一切使わない**。invoke_handler の 18 コマンドはまるごとフロント専用のエントリ点で、egui は下層の `_core`/engine 関数を直呼びする。撤去後、invoke_handler は空にできる（共有下層は残す。トレイの `open_settings`・instant 直呼び等は既に IPC 非経由）。
- **`webview2-com` + `windows-core 0.61` の依存は 3 関数に閉じる**（`suspend_webview` / `resume_webview` / `setup_accelerator_handler`・実測 grep）。一方 **`tauri` の `unstable` feature と wry ランタイムは egui 窓自体が要るため撤去後も残る**——「WebView2 撤去 ≠ Tauri 痩身」である。
- **隠れた順序制約**: flip した瞬間 `e2e:tauri`（WebView2 DOM 前提の Playwright）は成立しない。「flip だけ先行」の中間状態は WebView2 用逆フラグを新設しない限り CI が赤くなる。ゆえに egui smoke を先に立て、flip と同じ PR で e2e を切断する。

## 決定事項

### 決定 1: 3 段 PR。PR2/PR3 の境界は「コンパイラが検出できるか」で引く

一括 PR は review 不能な大きさ（ui/ 全体・npm 鎖・e2e・IPC 18 本・WebView2 Rust・conf/CSP・SPEC 約 22 行 + docs 索引）。PR1（smoke 新設）→ PR2（flip + コンパイラ視界内の撤去）→ PR3（視界外の撤去 + 文書同期）の 3 段とし、**中間状態も常に CI green** を保つ。PR2 に「dead_code が出るものすべて」を寄せることで、境界の判定を人の列挙でなく clippy `-D warnings` に委ねる（compile-fail を検出器にする既存規律の応用）。

### 決定 2: e2e 後継 = 「egui smoke（PR1）+ 手動 GUI smoke 体制」。#567 は obsolete として close

`e2e/` は WebView2 撤去で基盤ごと消える。後継の自動回帰は PR1 の smoke（下記）を最低線とし、手動 GUI smoke の体制（SU2 以来の実機スモーク列）を `docs/build-commands.md` に残す。**#567（WDIO embedded 移行）は移行対象の WebView2 が消えるため obsolete として close する**（本 spec マージ後・close 理由に本 spec を引く）。シナリオ拡充（egui の操作列自動化への投資）は必要が生じた時点で別 issue とする。

### 決定 3: smoke は trace 観測型（`smoke-startup.ps1` と同型・keybd_event 注入）

`scripts/smoke-egui.ps1` を新設: release ビルドを `SNOTRA_TRACE=1` で起動 → keybd_event でホットキー注入 → `egui_show:done` 観測 → Escape 注入 → `egui_hide:done` 観測 → プロセスツリーの `msedgewebview2.exe` 子孫 0 確認 → 終了。PR1 時点では `SNOTRA_EGUI_MAIN=1` を付けて現 main で green にし、**PR2 で env を外して「既定が egui であること」自体を CI の検証対象へ変える**。CI runner での keybd_event 到達は `e2e:tauri` が実窓で動いてきた実績から成立見込み（PR1 で実測確認・→「リスク」）。

## PR 構成

### PR1: egui smoke 新設（現 main で green・flip より先）

- `scripts/smoke-egui.ps1` + CI job（`e2e.yml` へ追加。paths に `src-tauri/**` 系を含める）。
- `docs/build-commands.md` に smoke の位置づけ（自動回帰の最低線）と手動 GUI smoke 体制を記載。
- マージ後に #567 を close（決定 2）。

### PR2: flip 本体 + コンパイラ視界内の撤去

- フラグ 7 地点の egui 無条件化・else 削除・`SNOTRA_EGUI_MAIN` 判定の除去。
- dead 化した WebView2 Rust コードを削り切る: `suspend_webview` / `resume_webview` / `setup_accelerator_handler` / `show_main_and_emit` / `reset_search_height` / `setup_window_geometry` / `sync_webview_background` / `suspend_and_trim_after_hide` と、`webview2-com`・`windows-core 0.61` 依存。dead_code warning が出たものは列挙外でも PR2 で消す（決定 1）。
- `tauri.conf.json` の宣言窓 "main" を除去し、実行時 `retain` ハックを削除。
- e2e 切断: `e2e/` 削除・`e2e.yml` の e2e job 削除（smoke job は残す）・`e2e-webview-automation` feature と `configure_e2e_webview` + テスト削除・ci.yml の feature-clippy job 削除・`playwright.tauri.config.ts` と package.json の `e2e:tauri*` scripts 削除。
- smoke から `SNOTRA_EGUI_MAIN` を外す（決定 3）。
- マージ後に実機 GUI smoke（既定起動で egui・検索/起動/folder/instant/tool/updater toast/設定サイドカーの一巡）。

### PR3: コンパイラ視界外の撤去 + 文書/SPEC 同期

- `ui/`（`ui/CLAUDE.md` 含む）・`dist`・`vite.config.ts` / `vitest.config.ts` / `tsconfig.json` / `typedoc.json` の削除、package.json のフロント工程 scripts（dev/build/test/typecheck/prebuild/docs:check/preview）とフロント専用 npm 依存の prune。**node 基盤（hooks・governance-check・githooks bootstrap・scripts）は残す。**
- invoke_handler 18 本と純フロント IPC ラッパーの削除（共有下層 `_core`/engine 関数・トレイ用 `pub` 関数は残置）・`capabilities/main.json`・CSP・`frontendDist` / `devUrl` / `beforeDevCommand` / `beforeBuildCommand` の整理。
- `release.yml` / `ci.yml` のフロント工程（npm ci・vite build・frontend-check job・rust-check 内 npm test）除去。release.yml の Verify / ZIP 工程は不変（snotra.exe + snotra-settings.exe）。
- **セーフティネット変更（本 spec の承認をもって合意とする・実施時は `.claude/rules/safety-nets.md` の手順で検証）**: `post-edit.mjs` の `ui/src/**` typecheck 割当・CSP 契約テスト割当の除去、`governance-check.mjs` の `ui:` エントリ整理、`.claude/rules/` の ui 配送規則の整理、hook-selftest / githooks-selftest の割当が壊れないことの確認。
- SPEC.md の両経路対比（約 22 行 + 関連節）を単一経路へ書き換え（**仕様変更として同期**——文書化された挙動の変更であり「fix」扱いにしない）。`src-tauri/CLAUDE.md` の WebView2 節群（生成制約・TrySuspend・EmptyWorkingSet の WebView2 部・AcceleratorKeyPressed・E2E feature）整理。`working_set.rs` は trim 自体を egui が使うため残し、子孫 BFS の説明を現状へ合わせる。AGENTS.md 3 層分担の `ui/src/**` 行・`docs/architecture.md` / `docs/build-commands.md` の該当節。

## 検証と受け入れ

- 各 PR で governance:check + CI green（沈黙経路の検査は post-edit hook の割当に従う。`*.md` は governance-check が CI で捕捉）。
- PR2 マージ後: 実機 GUI smoke 一巡 + smoke job が env なしで green。
- **PR3 マージ後: `npx tauri build --bundles nsis` が通り NSIS + ZIP が生成できること**——配布 spec のリリース手順 1（draft build）の前提。CI の release 経路は v0.19.0 の draft build で最終接地する。
- 完了で SU7 の実装部が閉じ、配布 spec のリリース手順（rig 検証 → Latest 化）へ接続する。#532 の close は v0.19.0 リリース完了時（配布 spec 手順 1〜6 完走）。

## スコープ外

- 配布・検証手順そのもの（配布 spec が正本）。
- `snotra-settings`（設定サイドカー・別プロセス egui）——無変更。
- egui 操作列の自動化拡充（必要時に別 issue・決定 2）。

## リスクと調査項目

- **`frontendDist` 無し（または空）で `tauri build` が成立するか未検証**——tauri の config schema / bundler がフロント資産を要求する可能性がある。**plan の最初に一次検証を置く**（空ディレクトリ指定・キー削除の両案を試す）。不成立なら空 `dist/` プレースホルダを残す fallback を採る。
- **CI での keybd_event 到達は PR1 で実測**——不達なら smoke を「起動 + trace 起動列観測 + msedgewebview2 子孫 0」へ縮退し、show/hide 列は手動 smoke へ残す（縮退しても SU7 受け入れ「smoke ≥1 本」は満たす）。
- **dead_code 検出器の限界**: `pub` シンボル・invoke_handler 登録済みコマンドは dead_code に出ない。PR3 の削除対象は Explore の依存面リスト（本 spec「背景」+ PR3 節）を列挙の起点にし、`cargo build` + grep で残参照ゼロを確認する。
- **SPEC 同期の規模**: 両経路対比の書き換えは削除より工数が要る。PR3 を文書だけで肥大させない——SPEC の節単位で「単一経路化」の書き換えに徹し、内容の再設計はしない。

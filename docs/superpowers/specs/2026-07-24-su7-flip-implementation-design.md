# SU7: flip 実装（既定 egui 化 + WebView2 撤去 + e2e 後継）設計

日付: 2026-07-24 / 対象: #532 Phase 2 SU7 のうち flip 実装と e2e 後継方針 / 先行: 配布・検証方針 spec（`2026-07-24-su7-distribution-verification-design.md`・PR #657 マージ済み）
ロードマップ: `2026-07-21-phase2-softbuffer-migration-roadmap.md` の SU7 行
スコープ確定: 配布 spec 決定 4（v0.18.3 = WebView2 最終版・v0.19.0 で flip + 撤去を同一リリース完結）を実装へ落とす。brainstorm で **e2e 後継方針も本 spec へ畳み込む**と決定（WebView2 撤去の確定により #567 の移行先が消え、「後継方針」の実体が「egui smoke を CI に立て #567 を閉じる」へ収束したため）。初版はマルチパースペクティブレビュー（事実照合 / 手順実行シミュレーション / 完全性・整合 / codex 敵対探索の 4 レンズ）を経て本改訂版へ更新した（決定 1 の弱化・PR 境界の再配置・keep-set の明示）。

## 背景と言語化

依存面の走査（Explore・2026-07-24）とレビュー接地で、構造の要点が確定した。

- **フラグ判定は製品コード 7 箇所に閉じる**（`main.rs` ×5: 宣言窓 retain / single-instance / setup 内 egui 初期化 / hotkey listener / `setup_startup_display`、`commands/window.rs` ×2: alwaysOnTop の get_window 分岐。walkthrough レンズが独立 grep で全数一致を確認）。いずれも素直な if/else で、flip は「egui 枝の無条件化 + else 削除」に帰着する。
- **egui 経路は IPC を一切使わない**。invoke_handler の 18 コマンドはまるごとフロント専用のエントリ点で、egui は下層の `_core`/engine 関数を直呼びする（`egui_shell/` に invoke 呼び出しゼロを実測確認）。撤去後、invoke_handler は空にできる（共有下層は残す。トレイの `open_settings`・instant 直呼び等は既に IPC 非経由）。
- **`webview2-com` + `windows-core 0.61` の依存は 3 関数に閉じる**（`suspend_webview` / `resume_webview` / `setup_accelerator_handler`・実測 grep）。一方 **`tauri` の `unstable` feature と wry ランタイムは egui 窓自体が要るため撤去後も残る**——「WebView2 撤去 ≠ Tauri 痩身」である。
- **隠れた順序制約 1（e2e）**: flip した瞬間 `e2e:tauri`（WebView2 DOM 前提の Playwright）は成立しない。「flip だけ先行」の中間状態は WebView2 用逆フラグを新設しない限り CI が赤くなる。ゆえに egui smoke を先に立て、flip と同じ PR で e2e を切断する。
- **隠れた順序制約 2（governance の三体結合）**: `governance-check` job は全 PR 無条件実行で、G5/G6 が **package.json scripts・workflow 本文・`docs/build-commands.md` を機械的に結合**している。script/workflow を消す PR は同一 PR で `docs/build-commands.md` の該当節（e2e 節・CI 対応表）を同期しないと必ず赤くなる。文書同期を PR3 へまとめる分担はこの 2 コマンド系文書には適用できない。
- **「フロント専用」に見えて実はセーフティネットの実行系である依存がある**: `npm test` = `vitest run` は ui だけでなく `.claude/hooks` / `.githooks` / `scripts` のテストを走らせる共通ランナーで、post-edit の hook-selftest / githooks-selftest も vitest バイナリを直接起動する。`@tauri-apps/cli` も NSIS ビルド（配布 spec 手順 1）に必須。**prune は「フロント専用の列挙」でなく「keep-set の明示」から始める**（→ PR3）。
- 走査の限界の教訓: Explore の依存面リストは `sync_webview_background` の config_watcher 側呼び出しを 1 件見落としていた（codex が検出）。**削除リストは列挙を起点にしつつ、削除前に必ず grep で全参照を検算する**。

## 決定事項

### 決定 1: 3 段 PR。中間状態も常に CI green。clippy は境界の「検出器の一つ」であり仲裁者ではない

一括 PR は review 不能な大きさ（ui/ 全体・npm 鎖・e2e・IPC 18 本・WebView2 Rust・conf/CSP・SPEC + docs 索引）。PR1（smoke 新設）→ PR2（flip + Rust 撤去）→ PR3（視界外の撤去 + 文書同期）の 3 段とする。

初版の「境界判定を clippy `-D warnings` に委ねる」はレビューで**3 種の盲点**が実証されたため弱める: (a) invoke_handler 登録コマンドから届く生存連鎖（`notify_main_hidden` → `suspend_and_trim_after_hide` → `suspend_webview`）は PR3 まで dead にならない、(b) setup() から**無条件に**呼ばれる関数（`setup_window_geometry` / `sync_webview_background` / `setup_accelerator_handler`）は呼び出し行を人手で消すまで dead にならない、(c) 文字列リテラルの emit（config_watcher の値運搬 7 本）はそもそもコンパイラの視界外。**境界は本 spec の割当リストが定め、clippy dead_code は「割当漏れの検出器」として使う**（dead_code が出たら PR2 で消す、は維持）。各削除は実施前に grep で全参照を検算する。

### 決定 2: e2e 後継 = 「egui smoke（PR1）+ 手動 GUI smoke 体制」。#567 は obsolete として close

`e2e/` は WebView2 撤去で基盤ごと消える。後継の自動回帰は PR1 の smoke（下記）を最低線とし、手動 GUI smoke の体制（SU2 以来の実機スモーク列）を `docs/build-commands.md` に残す。**#567（WDIO embedded 移行）は移行対象の WebView2 が消えるため obsolete として close する**(本 spec マージ後・close 理由に本 spec を引く)。シナリオ拡充（egui の操作列自動化への投資）は必要が生じた時点で別 issue とする。

### 決定 3: smoke は trace 観測型（keybd_event 注入・自前ビルド・config seed）

`scripts/smoke-egui.ps1` を新設: release ビルドを `SNOTRA_TRACE=1` で起動 → keybd_event でホットキー注入 → `egui_show:done`（`egui_shell/mod.rs` 実在確認済み）観測 → Escape 注入 → `egui_hide:done` 観測 → プロセスツリーの `msedgewebview2.exe` 子孫 0 確認 → 終了。設計の確定点:

- **注入列は Alt+Q（config.rs の既定 hotkey）で、Alt の解放を含める**——Alt 押下中は `ShowAfterAltRelease`（最大 350ms 遅延）に回るため、Alt up 送出後に観測するか 350ms 超の待ちを置く。
- **最小の config.toml を seed してから起動する**——CI runner は config 不在 = first-run 経路に入り、`snotra-settings --first-run` の spawn がフォーカスを奪って観測を壊しうる。first-run フローの検証は smoke の責務外。
- **smoke job は自前の release ビルド工程を持つ**——現行 smoke step のビルドは `e2e:tauri:setup`（PR2 で消える）に依存しており継承できない。
- PR1 時点では `SNOTRA_EGUI_MAIN=1` を付けて現 main で green にし、**PR2 で env を外して「既定が egui であること」自体を CI の検証対象へ変える**。
- CI runner での keybd_event 到達は `e2e:tauri` が実窓で動いてきた実績から成立見込み（PR1 で実測・→「リスク」）。

## PR 構成

### PR1: egui smoke 新設（現 main で green・flip より先）

- `scripts/smoke-egui.ps1`（決定 3）+ CI job（`e2e.yml` へ追加・自前 release ビルド工程・**`scripts/smoke-egui.ps1` を e2e.yml の paths 自己参照へ追加**）。
- `docs/build-commands.md` の同期（G5/G6 対応: smoke の位置づけ・CI 対応表への行追加）と手動 GUI smoke 体制の記載。
- マージ後に #567 を close（決定 2）。

### PR2: flip 本体 + Rust コードの撤去

- フラグ 7 地点の egui 無条件化・else 削除・`SNOTRA_EGUI_MAIN` 判定の除去。
- **else 枝の削除で dead 化するもの**を削る: `show_main_and_emit` / `reset_search_height` / `resume_webview`（呼び出しが全て else 枝にあることは walkthrough レンズが確認済み）。
- **無条件呼び出しの 3 関数は「呼び出し行の削除 → dead 化 → 関数削除」の順で**消す: `setup_window_geometry` / `setup_accelerator_handler` / `sync_webview_background`。後者は **setup 内と config_watcher の visual-config 反映経路の 2 箇所**から呼ばれる——config_watcher 側は呼び出し行と webview 幅反映ブロック（`get_webview_window` 依存）を合わせて削る。
- `webview2-com`・`windows-core 0.61` 依存の除去（上記 3 関数の削除で参照ゼロ・grep で検算）。
- **`suspend_and_trim_after_hide` / `suspend_webview` は PR2 で削らない**——invoke_handler 登録の `notify_main_hidden` から生存しており、PR3 の IPC 撤去と同時に消す（決定 1 の盲点 (a)）。
- `tauri.conf.json` の宣言窓 "main" を除去し、実行時 `retain` ハックを削除。
- e2e 切断: `e2e/` 削除・`e2e.yml` の e2e job 削除（smoke job は残す）・`e2e-webview-automation` feature と `configure_e2e_webview` + テスト削除・**ci.yml の rust-check 内 feature-clippy step**（job ではない）削除・`playwright.tauri.config.ts` と package.json の `e2e:tauri*` scripts 削除。
- **`docs/build-commands.md` の e2e 節・CI 対応表の該当行を同一 PR で同期する**（G5/G6 の三体結合・順序制約 2）。post-edit hook の `e2e/**` typecheck 割当と tsconfig の `e2e` include も e2e/ 削除と同時に外す（検査対象と検査の同時整合・#489）。
- required check に e2e job 名が登録されている場合は GitHub 側設定の更新が要る（repo 外設定・実施時に確認)。
- smoke から `SNOTRA_EGUI_MAIN` を外す（決定 3）。
- マージ後に実機 GUI smoke（既定起動で egui・検索/起動/folder/instant/tool/updater toast/設定サイドカーの一巡）。

### PR3: コンパイラ視界外の撤去 + 文書/SPEC 同期

**npm 層 — prune は keep-set から**:

- **keep-set（明示保持）**: `vitest`（hook-selftest / githooks-selftest / `npm test` の実行系）・`@tauri-apps/cli`（NSIS ビルド）・`prepare` 鎖（githooks bootstrap）・`governance:check` / `hook-selftest` / smoke / measure 系 scripts とその依存。
- `vitest.config.ts` は**削除でなく trim**——ui include を外し、`.claude/hooks/**` / `.githooks/**` / `scripts/**` の include を残す（bare `vitest run` のスコープを既定 glob へ広げない）。
- `npm test` は **CI に残す**——`frontend-check` job は node-check（npm ci + npm test〔hooks/githooks/scripts スコープ〕）へ縮退させ、rust-check 内 Windows `npm test` も残置（ubuntu/Windows 相補実行の趣旨は ci.yml コメントのとおり維持）。
- prune 対象（真にフロント/e2e 専用）: `solid-js` / `vite` / `vite-plugin-solid` / `@solidjs/testing-library` / `jsdom` / `typedoc` / `@playwright/test` / `selenium-webdriver` / `edgedriver` / `@tauri-apps/api` / `@tauri-apps/plugin-updater`（npm 側）等。package.json のフロント工程 scripts（dev/build/test:ui 相当/typecheck/prebuild/docs:check/preview）整理。

**ファイル・設定**:

- `ui/`（`ui/CLAUDE.md` 含む）・`dist`・`vite.config.ts`・`tsconfig.json`・`typedoc.json` の削除。**編集順**: 先に `post-edit.mjs` から csp-test / typecheck の割当を外し（hook-selftest green を確認）、その後に ui/ と conf を触る——検査対象を変更しながら検査を走らせない（#489）。
- invoke_handler 18 本と純フロント IPC ラッパーの削除（共有下層 `_core`/engine 関数・トレイ用 `pub` 関数は残置）。**このとき `notify_main_hidden` 連鎖の `suspend_and_trim_after_hide` / `suspend_webview` を一緒に消す**（PR2 からの送り）。
- **config_watcher の値運搬 emit 7 本**（`language-changed` / `visual-config-changed` / `show-icons-changed` / `auto-hide-focus-lost-changed` / `max-results-changed` / `instant-prefix-changed` / `top-n-history-changed`——egui 側リスナー無し・文字列 emit ゆえコンパイラ視界外）と `window-shown` / `window-hidden` emit の撤去。`config-applied` / indexing 系 / `hotkey-registration-failed` / `platform-event` / `egui-hide-requested` / `exit-requested` / `open-settings` は egui・トレイが使うため残す。
- `capabilities/main.json`・CSP・`frontendDist` / `devUrl` / `beforeDevCommand` / `beforeBuildCommand` の整理（→ 前提ゲート）。
- `release.yml` / `ci.yml` のフロント工程（npm の vite build 工程・frontend-check の縮退）。release.yml の Verify / ZIP 工程は不変（snotra.exe + snotra-settings.exe）。`npm ci` は keep-set が要るため残る。

**セーフティネット変更（本 spec の承認をもって合意とする・実施時は `.claude/rules/safety-nets.md` の手順で検証）**:

- `post-edit.mjs`: `ui/src/**` typecheck・CSP 契約テスト割当の除去（tsconfig / cspValidation.test.ts の削除と同時・上記編集順）。hook-selftest / githooks-selftest が **vitest 残置の下で** green を維持することを確認。`post-edit.test.mjs` の期待（tsconfig include・vitest include の契約）を新構成へ更新。
- `governance-check.mjs`: `ui:` エントリ整理（`ui/CLAUDE.md` 削除と同時）。
- `.claude/rules/` の ui 配送規則の整理。

**文書/SPEC 同期（governance が意味的 stale を捕捉しない層を含む）**:

- SPEC.md の両経路対比を単一経路へ書き換え（**仕様変更として同期**）。`SNOTRA_EGUI_MAIN` に言及するライブ文書（SPEC.md・`src-tauri/CLAUDE.md`・`docs/build-commands.md`）を grep して単一経路化（docs/superpowers 配下の specs/plans は時系列記録ゆえ触らない）。
- `src-tauri/CLAUDE.md`: WebView2 節群（生成制約・TrySuspend・EmptyWorkingSet の WebView2 部・AcceleratorKeyPressed・E2E feature）と **config_watcher「発火するイベント」列挙**の同期。`working_set.rs` は trim 自体を egui が使うため残し、子孫 BFS の説明を現状へ合わせる。
- **ルート CLAUDE.md のフック表**（ui/src・e2e・CSP 契約テスト・tsconfig include の記述）と **AGENTS.md** の「機能削除・IPC ルート変更 → e2e/ を grep」トリガー行・ドキュメント参照の ui 索引行・3 層分担の `ui/src/**` 行。`docs/architecture.md` / `docs/hooks.md` の該当節。

## 検証と受け入れ

- 各 PR で governance:check + CI green（G5/G6 の三体結合ゆえ、scripts/workflow/docs/build-commands.md は常に同一 PR で同期する）。
- PR2 マージ後: 実機 GUI smoke 一巡 + smoke job が env なしで green。
- **PR3 マージ後: `npx tauri build --bundles nsis` が通り NSIS + ZIP が生成でき、hook-selftest / githooks-selftest / `npm test`（hooks/githooks/scripts スコープ）が green であること**。
- 完了で SU7 の実装部が閉じ、配布 spec のリリース手順（rig 検証 → Latest 化）へ接続する。#532 の close は v0.19.0 リリース完了時（配布 spec 手順 1〜6 完走）。

## スコープ外

- 配布・検証手順そのもの（配布 spec が正本）。
- `snotra-settings`（設定サイドカー・別プロセス egui）——無変更。
- egui 操作列の自動化拡充（必要時に別 issue・決定 2）。

## リスクと調査項目

- **フロント資産なしの `tauri build` 成立性は PR3 の前提ゲート**——(1) `frontendDist` 無し/空、(2) 宣言窓ゼロ（`windows: []`）の**静的**構成（実行時 retain 除去の動作実績はあるが build 時は未接地）。plan の最初に一次検証を置き、不成立なら空 `dist/` プレースホルダ + 最小キー残置の fallback を採る。この検証が通るまで PR3 の conf 撤去には着手しない。
- **CI での keybd_event 到達は PR1 で実測**——不達なら smoke を「seed config で起動 + trace 起動列観測 + msedgewebview2 子孫 0」へ縮退し、show/hide 列は手動 smoke へ残す（縮退しても SU7 受け入れ「smoke ≥1 本」は満たす）。
- **削除の検算**: 各削除の前に grep で全参照を確認する（Explore 走査に 1 件の漏れがあった実績・決定 1）。dead_code / compile error は割当漏れの検出器として使い、出たら本 spec の割当へ照らして PR2/PR3 いずれかに編入する。
- **SPEC 同期の規模**: 両経路対比の書き換えは削除より工数が要る。PR3 を文書だけで肥大させない——SPEC の節単位で「単一経路化」の書き換えに徹し、内容の再設計はしない。

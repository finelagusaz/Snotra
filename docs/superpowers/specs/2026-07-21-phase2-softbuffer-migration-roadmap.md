# Phase 2 ロードマップ — メインウィンドウの softbuffer egui 移行（#532）

- 種別: 分解ロードマップ（各サブユニットは独立の spec → plan → 実装サイクルを持つ）
- 日付: 2026-07-21
- 親: #532（メインウィンドウの egui/softbuffer 移行）

## 背景と位置づけ

Phase 1（技術スパイク + 採用ゲート検証）は完了した。スパイク `snotra-egui-mvp`（`soft_host`）で、Tauri 管理ウィンドウへ WebView なしで softbuffer CPU ラスタライズの egui を描画し、実 `Engine`・Alt+Q・IME・Rust Updater・3,000 回耐久まで通した。採用ゲートは:

- #581 コールドスタート内訳、#582 IME 実操作、#579 異 DPI 実機 — 通過・close（2026-07-21）
- #580 署名付き実更新 — ローカル可能範囲（3 更新モード・終了保存）を検証、核心（署名実更新・署名 artifact・install/uninstall）は本番鍵 + CI/隔離環境が必須で open 継続
- メモリゲート・外観維持は #532「構成確定（2026-07-18）」で通過

**Phase 2 = 製品 `src-tauri` のメインウィンドウを、WebView2 + SolidJS + IPC から softbuffer egui + 直 `Engine` 呼びへ移行する。** 設定サイドカー `snotra-settings` は別プロセス（eframe/glow）のまま維持する。

## アプローチ（決定済み）

- **フラグ並行移行**: `src-tauri` に egui メイン経路を WebView2 と並行構築し、env/フラグでウィンドウ生成時に経路を選択する。egui と WebView2 はレンダリング経路が異なり「半分だけ egui」はできないため、切替はウィンドウ生成時の二択になる。WebView2 を既定に保ちつつ egui をドッグフードし、parity 到達で既定を egui へ切替える。製品は移行中も常に出荷可能。
- **描画基盤の隔離**: `snotra-egui-runtime` を wgpu → softbuffer へ **in-place 置換**し、製品非依存の描画/ウィンドウ/IME 基盤クレートとする。`EguiRuntime` が `EguiView` を駆動する既存 API を維持する。`src-tauri` は `EguiView` を実装して製品 UI/状態/updater-glue を持つ。

## 決定事項

1. **wgpu は却下済み**（#532 採用判断ゲート「起動・待機時メモリが悪化しない」= 不合格、「現行 wgpu 構成の製品採用: No-Go」）。GPU ドライバ固定費（wgpu ~469MiB）が WebView2 固定費（168MiB）と同格でメモリ削減目的に効かず、softbuffer へ転換した根拠そのもの。softbuffer の対 WebView2 メモリ優位は検証済み（PrivWS で hidden+trim ~17×・visible ~4.3×）。**残して比較すべき未実施の計測は無い。** → SU1 で `snotra-egui-runtime` を softbuffer へ置換し、却下済み wgpu/glow の probe bin（`main.rs`・`glow_main.rs`・`glow_lifecycle_main.rs`・`glow_park_host_main.rs`）を撤去する。検証記録は #532 コメント + git 履歴に残る。
2. **再変換（IME reconvert）は WANT**（nice-to-have）。egui_winit/winit は `WM_IME_REQUEST`（`IMR_RECONVERTSTRING`）を提供しないため独自 IMM32 実装が要る（#582）。**切替をブロックしない。** SU1/SU5 で低コストに載れば実装、困難なら defer。
3. **`fill_mesh` の AA 品質**（テキスト主体のランチャー）は SU1 の受け入れ条件で製品規模の文字品質として検証する。#399/#579 のベースライン顕在化はこの被覆 AA 欠如が前触れだった。

## spec 分割（SU1–SU7）

各サブユニットは独立の spec → plan → 実装サイクルを持つ。

| SU | 内容 | 境界 | 主参照 |
|---|---|---|---|
| **SU1 softbuffer runtime クレート** | `snotra-egui-runtime` の renderer/surface/gpu を softbuffer + `fill_mesh` へ置換。`EguiRuntime`/`EguiView` API 維持、ime/input/runtime/repaint 流用。被覆 AA 品質・フォント単一化を作り込む。wgpu/glow probe 撤去 | クレート | — |
| **SU2 ウィンドウシェル + 状態機械** | フラグ選択の egui ウィンドウ生成（WebView2 と並行）、Alt+Q 表示/非表示・blur 非表示・フォーカス列・残留 Alt 解除・位置永続・起動時表示・初回フロー | `src-tauri` | SPEC §8（8.1–8.6）, §18.5 |
| **SU3 検索体験**（最大） | クエリ + IME・インクリメンタル検索（**直 `Engine`**・IPC 撤去）・結果リスト/行（アイコン + 名前 + 淡色パス）・キーボードナビ・選択・フォルダ展開・インスタントコマンド | `src-tauri`（`EguiView` 実装） | SPEC §4, §4.7, §19.5, §19.7 |
| **SU4 アイコン** | 実アイコン抽出 + LRU キャッシュ + 非同期バッチを Rust へ（IPC icon コマンド + フロント `lruIconCache`/`iconBatch` を置換） | `src-tauri` | — |
| **SU5 updater** | check/download/install・保存優先（`downloadAndInstall` 復帰後に保存を置かない）・toast 相当・3 モード（full/check_only/disabled）・relaunch。再変換（WANT）はここで可能なら | `src-tauri` | SPEC §20（20.2, 20.4） |
| **SU6 統合 glue** | `config_watcher` 反映（テーマ/ホットキー/index を egui ウィンドウへ）・終了保存（`setup_exit_listener` 整合）・設定サイドカー共存 | `src-tauri` | — |
| **SU7 配布 + 切替** | 署名付き NSIS/updater artifact（#580 の CI/隔離環境）・portable ZIP 判断・**既定を egui へ切替 + WebView2 経路撤去** | 設定 + CI | #580 |

## 依存順・並行

```
SU1 → SU2 → ( SU3 検索本体  ∥  SU4 アイコン ) → SU5 → SU6 → SU7（切替）
```

- SU1 が全体の基盤。SU2 がフラグ並行の器を作る。
- SU3（検索本体）が最大で、SU4（アイコン）は SU2 後に並行しうる。
- SU7 の「切替」は下記 flip 基準を全て満たしてから。

## 切替（flip）基準 — egui を既定にする条件

1. SPEC §4 / §4.7 / §8 / §20 の挙動が egui 経路で parity（SU2–SU6 の受け入れ達成）
2. **外観維持**（#532 目標・スパイクで検証済）— 製品ダークテーマ・フォント・行レイアウトの目視 parity
3. **メモリゲート維持** — release の visible / hidden+trim の PrivWS が WebView2 版以下（検証済の水準を製品規模で再確認）
4. 採用ゲート通過 — #579/#581/#582（済）+ #580 核心（CI）
5. WebView2 子孫プロセス 0・`app.windows` が egui 経路で空

## 各 SU の受け入れ条件（要旨）

- **SU1**: `EguiView` を差し替えるだけで任意 UI を描画できる。フォント単一化（jp_font 先頭・`snotra-egui-mvp/CLAUDE.md` 不変条件）と被覆 AA 品質が製品規模のテキストで目視 parity。IME preedit/候補/確定が softbuffer 上で正しい。
- **SU2**: Alt+Q・blur・フォーカス列・位置永続・起動時/初回が SPEC §8 と一致。フラグ off で WebView2 挙動は不変（回帰なし）。
- **SU3**: インクリメンタル検索の不変条件（SPEC §4.2.1）・優先順位（§4.3）・結果表示制御（§4.7）・インスタントコマンド・フォルダ展開が IPC なしで一致。
- **SU4**: アイコン抽出/キャッシュ/非同期が現行と同等（欠落時プレースホルダ・N 件上限の下流整合）。
- **SU5**: 3 モードの gating が現行フロント（`MainApp.tsx`）と一致。保存優先・relaunch が壊れない。
- **SU6**: config 変更の反映（テーマ/ホットキー/index）・終了保存・サイドカーが egui ウィンドウで動く。
- **SU7**: 署名付き実更新・install/uninstall が CI/隔離環境で通り、切替後 WebView2 経路が撤去され回帰がない。

## リスク

- **`fill_mesh` の被覆 AA 品質**: 自作 CPU ラスタライザは被覆率 AA を持たない（ピクセル中心二値判定）。製品はテキスト主体ゆえ品質が UX 直結。SU1 で AA を作り込むか、品質が十分かを製品規模で検証する。
- **再変換の IMM32 実装**: winit が `WM_IME_REQUEST` を握るため、生 Win32 メッセージへのフックが要る。WANT ゆえ切替をブロックしないが、実装は非自明。
- **二経路の並行維持コスト**: WebView2 + egui のフラグ境界をウィンドウ生成時に限定し、二重メンテを薄く保つ。並行期間を短く（parity を早期に）する。
- **Tauri 内部 API 追随**: unstable feature / `tauri-runtime-wry` への依存。バージョン更新時に追随コストが残る（#532 既知）。

## 進め方

本ロードマップは Phase 2 の分解であり、実装計画ではない。**各 SU が独立の spec → plan → 実装サイクル**を持つ。次は **SU1（softbuffer runtime クレート）を brainstorm** して spec 化する。SU 間で同一ファイルに触れる並行作業は境界を確認してから（ルート `CLAUDE.md`「サブエージェント委譲と worktree」）。

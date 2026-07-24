# Phase 2 ロードマップ — メインウィンドウの softbuffer egui 移行（#532）

- 種別: 分解ロードマップ（各サブユニットは独立の spec → plan → 実装サイクルを持つ）
- 日付: 2026-07-21（改訂: 2026-07-23 — SU1–SU3 完了を受け SU4 以降を再編成し、follow-up issue を各 SU へ割当）
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

1. **wgpu は却下済み**（#532 採用判断ゲート「起動・待機時メモリが悪化しない」= 不合格、「現行 wgpu 構成の製品採用: No-Go」）。GPU ドライバ固定費（wgpu ~469MiB）が WebView2 固定費（168MiB）と同格でメモリ削減目的に効かず、softbuffer へ転換した根拠そのもの。softbuffer の対 WebView2 メモリ優位は検証済み（PrivWS で hidden+trim ~17×・visible ~4.3×）。**残して比較すべき未実施の計測は無い。** → SU1 で `snotra-egui-runtime` を softbuffer へ置換し、却下済み wgpu/glow の probe bin（`glow_main.rs`・`glow_lifecycle_main.rs`・`glow_park_host_main.rs`）を撤去する。**`main.rs` は当初この撤去対象に含めたが、SU1 spec（`2026-07-21-su1-softbuffer-runtime-design.md`）で「softbuffer runtime 駆動 probe への転用」へ改定した**——素直に撤去すると softbuffer ランタイムを end-to-end で動かす harness が消え、SU1 が AA/IME を自証できなくなるため。検証記録は #532 コメント + git 履歴に残る。
2. **再変換（IME reconvert）は WANT**（nice-to-have）。egui_winit/winit は `WM_IME_REQUEST`（`IMR_RECONVERTSTRING`）を提供しないため独自 IMM32 実装が要る（#582）。**切替をブロックしない。** SU1/SU5 で低コストに載れば実装、困難なら defer。
3. **`fill_mesh` の AA 品質**（テキスト主体のランチャー）は SU1 の受け入れ条件で製品規模の文字品質として検証する。#399/#579 のベースライン顕在化はこの被覆 AA 欠如が前触れだった。

## 進捗（2026-07-23 改訂時点）

- **SU1 完了** — `snotra-egui-runtime` の softbuffer 置換（PR #627）。follow-up: #628（render perf・非ブロッキング → SU6.5 で扱いを判断）
- **SU2 完了** — ウィンドウシェル + 状態機械（PR #629・main マージ 68b5f41）
- **SU3 完了** — M1 機能中核（PR #630）/ M2 folder 展開（PR #636）/ M3 instant+slash（PR #637）。いずれも main マージ済。follow-up: #631/#632/#633/#634/#638（下表の各 SU へ割当済）
- **SU3.5 完了** — tool-selection（PR #641）。#638 先勝ち正規化を同時解消
- **SU4 完了** — アイコン + 視覚 pass + §11 テーマ消費（PR #644）
- **SU5 完了** — updater + 通知 primitive + #631（起動 async 化・single-flight・flush-on-Enter）（PR #647・#631 close）。保存は plugin の `on_before_exit` hook で構造化（Windows では `downloadAndInstall` が復帰せず exit(0) する一次確認に基づく——現行 WebView2 経路の「update 時に終了保存が走らない」既存 gap も同時解消）。hidden 中 drain の要石（spec C 節）は実装時スモークで決着——egui_shell 経路では tao が hidden 窓へ update() を配らず（実測）、reset-on-show の backstop が launching/一時通知を確実にクリアする。回帰スモーク 8 項目 + ユーザー視覚スモーク合格。follow-up は #648（wake 恒久化 + doc/dead-field 整理）へ
- 残り: `SU6 → SU6.5 → SU7`（→「依存順・並行」）

## spec 分割（SU1–SU7）

各サブユニットは独立の spec → plan → 実装サイクルを持つ。

| SU | 内容 | 境界 | 主参照 |
|---|---|---|---|
| **SU1 softbuffer runtime クレート** | `snotra-egui-runtime` の renderer/surface/gpu を softbuffer + `fill_mesh` へ置換。`EguiRuntime`/`EguiView` API 維持、ime/input/runtime/repaint 流用。被覆 AA 品質・フォント単一化を作り込む。wgpu/glow probe 撤去 | クレート | — |
| **SU2 ウィンドウシェル + 状態機械** | フラグ選択の egui ウィンドウ生成（WebView2 と並行）、Alt+Q 表示/非表示・blur 非表示・フォーカス列・残留 Alt 解除・位置永続・起動時表示・初回フロー | `src-tauri` | SPEC §8（8.1–8.6）, §18.5 |
| **SU3 検索体験**（最大） | クエリ + IME・インクリメンタル検索（**直 `Engine`**・IPC 撤去）・結果リスト/行（アイコン + 名前 + 淡色パス）・キーボードナビ・選択・フォルダ展開・インスタントコマンド | `src-tauri`（`EguiView` 実装） | SPEC §4, §4.7, §19.5, §19.7 |
| **SU3.5 tool-selection** | カスタムオープナーのツール選択メニュー（§18）。`Option<FolderFrame>` を `tool` を積める view stack へ一般化（「第二の事例が現れた時点で一般化する」規律・SU3 spec 決定5）。**#638**（instant 重複名の first-match）を core 隣接作業として相乗り——config ロード時正規化（案 A）で表示と実行の不一致を表現不能にする | `src-tauri` | SPEC §18, #638 |
| **SU4 アイコン + 視覚 pass** | 実アイコン抽出 + LRU キャッシュ + 非同期バッチを Rust へ（IPC icon コマンド + フロント `lruIconCache`/`iconBatch` を置換）。**#632（行 legibility・truncate・scroll 追従 gate）を同一 pass に統合**——同じ `draw_result_row` を二度作り直さない。**テーマ値の消費（§11 parity）もここで作る**——ハードコード色でなく config テーマ（背景/入力欄/テキスト/選択行/ヒント色・フォント）から描く。これが無いと SU6 の「テーマ反映」に書き込む先が無い。非同期バッチは folder 展開の per-nav thread + drain 最新 token パターンを踏襲 | `src-tauri` | SPEC §3.4, §11, #632 |
| **SU5 updater + 通知** | **汎用通知 primitive（toast 相当）を先に設計**し、updater（check/download/install・保存優先＝`downloadAndInstall` 復帰後に保存を置かない・3 モード full/check_only/disabled・relaunch）と **#631（起動の async 化 + single-flight + 失敗通知）**を同じ primitive に載せる。flush-on-Enter 乖離（M1 起因・Plain の trailing 窓内 Enter が leading 結果で起動しうる・未起票）は #631 へ追記して同時解消。再変換（WANT）はここで可能なら | `src-tauri` | SPEC §20（20.2, 20.4）, §19.6, #631 |
| **SU6 統合 glue** | `config_watcher` 反映（テーマ/ホットキー/index を egui ウィンドウへ）・**#633（async 再インデックス中の stale 結果クリア・§4.7）**・**§12 IME 制御（設定有効時の表示時 IME オフ）の egui parity**・終了保存（`setup_exit_listener` 整合）・設定サイドカー共存 | `src-tauri` | SPEC §12, §4.7, #633 |
| **SU6.5 flip 前ハードニング** | flip 基準の実測群を一括実行: メモリゲート製品規模再測（基準 3・同日ペア測定）・外観目視 parity（基準 2）・**#628 の hidden 時挙動確認**（アイドル再描画が hidden で止まるか。止まらないなら flip ブロッカーへ昇格——常駐ランチャーの CPU/電力に直結）・`e2e:tauri` CI 通過確認・**#652 起動時 hotkey 登録失敗通知の egui 受け口**（`platform-event` の `initial-hotkey-failed`——未対応だと egui モードで窓を開く手段が無いのに無通知。PR #651 の code-review が検出・SU6 の pending 受け口と同型で移植、窓強制表示 parity の要否が主論点） | 計測 + 検証 | #628, #652 |
| **SU7 配布 + 切替** | 署名付き NSIS/updater artifact（#580 の CI/隔離環境）・portable ZIP 判断・**`e2e/` の後継方針決定**（WebView2 撤去で基盤喪失・#567 との順序整理・egui 経路の自動回帰 smoke の定式化）・**既定を egui へ切替 + WebView2 経路撤去** | 設定 + CI | #580, #567 |

## 依存順・並行

```
SU1 → SU2 → SU3 → #634 G-SYNC 実測 → SU3.5 → SU4 → SU5 → SU6 → SU6.5 → SU7（切替）
```

- **#634（G-SYNC 実測）を SU3.5 より先に置く**: 「同期直 `Engine` 呼びが並行機構を崩壊させる」という SU3 の要石命題の、最後の未検証部分（大インデックスでのフレームコスト）。詰まる場合の `spawn_blocking` + token 化は view 設計に波及するため、SU3.5/SU4 を上に積む前に測る。
- **SU3.5 と SU4 は直列**（初版の「SU3 ∥ SU4 並行しうる」を supersede）: 双方が `view.rs` に触れるためファイル境界が衝突する（ルート `CLAUDE.md`「サブエージェント委譲と worktree」）。SU3 の文脈が温かいうちに SU3.5 を先に通す。
- SU7 の「切替」は下記 flip 基準を全て満たしてから。

## 切替（flip）基準 — egui を既定にする条件

1. SPEC §4 / §4.7 / §8 / §20 の挙動が egui 経路で parity（SU2–SU6 の受け入れ達成）
2. **外観維持**（#532 目標・スパイクで検証済）— 製品ダークテーマ・フォント・行レイアウトの目視 parity
3. **メモリゲート維持** — release の visible / hidden+trim の PrivWS が WebView2 版以下（検証済の水準を製品規模で再確認）
4. 採用ゲート通過 — #579/#581/#582（済）+ #580 核心（CI）
5. WebView2 子孫プロセス 0・`app.windows` が egui 経路で空

## 各 SU の受け入れ条件（要旨）

- **SU1**: `EguiView` を差し替えるだけで任意 UI を描画できる。フォント単一化（jp_font 先頭・`snotra-egui-mvp/CLAUDE.md` 不変条件）と被覆 AA 品質が製品規模のテキストで目視 parity。IME preedit/候補/確定が softbuffer 上で正しい。
- **SU2**: Alt+Q・blur・フォーカス列・位置永続・起動時/初回が SPEC §8 と一致。フラグ off で WebView2 挙動は不変（回帰なし）。**Alt+Q のホットキー分岐と UI コマンド適用は純関数として保ち、冪等性（表示中+Show／非表示中+Hide）と show 進行中に届いた Hide の繰り延べをテストで固定する**（SU1 spike の `plan_hotkey`/`plan_ui_action` で実証済み・撤去済みバイナリからの申し送り）。
- **SU3**: インクリメンタル検索の不変条件（SPEC §4.2.1）・優先順位（§4.3）・結果表示制御（§4.7）・インスタントコマンド・フォルダ展開が IPC なしで一致。
- **SU3.5**: §18 ツール選択メニュー（Shift+Enter・folder の上への積層・Escape/復帰列）が egui 経路で parity。stack 一般化後も folder/results の既存挙動が回帰しない。#638 の正規化で「候補リストの表示と実行される action の不一致」が表現不能になる（両経路共通の core レベル）。
- **SU4**: アイコン抽出/キャッシュ/非同期が現行と同等（欠落時プレースホルダ・N 件上限の下流整合）。行視覚が #632 の症状（name/path 重なり・毎フレーム scroll_to_me）を解消。**色・フォントが config テーマ値から描かれ、ハードコード色が残らない**（§11 parity）。非同期アイコンが stale 描画を起こさない（token drain）。
- **SU5**: 3 モードの gating が現行フロント（`MainApp.tsx`）と一致。保存優先・relaunch が壊れない。通知 primitive が updater 通知と起動失敗通知（#631）の両方を表示し、起動が UI スレッドを止めず（dead UNC・モーダルダイアログ）、in-flight 窓の二重起動もしない（single-flight）。
- **SU6**: config 変更の反映（テーマ/ホットキー/index）・終了保存・サイドカーが egui ウィンドウで動く。セッション中の再インデックスで stale 結果が消える（#633・§4.7）。§12 IME 制御が egui 経路で機能する。
- **SU6.5**: メモリゲート再測が flip 基準 3 を満たす。hidden 時に再描画が止まることを実測確認（止まらないなら #628 を flip ブロッカーへ昇格）。外観目視 parity 合格。起動時 hotkey 登録失敗が egui 経路でも通知される（#652）。
- **SU7**: 署名付き実更新・install/uninstall が CI/隔離環境で通り、切替後 WebView2 経路が撤去され回帰がない。**`e2e/` の後継方針が決定済みで、egui 経路の自動回帰 smoke（keybd_event 注入 + `SNOTRA_TRACE` の `egui_show:done`/`egui_hide:done` 観測の定式化等）が最低 1 本 CI で回る**。

## リスク

- **`fill_mesh` の被覆 AA 品質**: 自作 CPU ラスタライザは被覆率 AA を持たない（ピクセル中心二値判定）。製品はテキスト主体ゆえ品質が UX 直結。SU1 で AA を作り込むか、品質が十分かを製品規模で検証する。
- **再変換の IMM32 実装**: winit が `WM_IME_REQUEST` を握るため、生 Win32 メッセージへのフックが要る。WANT ゆえ切替をブロックしないが、実装は非自明。
- **二経路の並行維持コスト**: WebView2 + egui のフラグ境界をウィンドウ生成時に限定し、二重メンテを薄く保つ。並行期間を短く（parity を早期に）する。
- **Tauri 内部 API 追随**: unstable feature / `tauri-runtime-wry` への依存。バージョン更新時に追随コストが残る（#532 既知）。
- **e2e 資産の基盤喪失**: WebView2 撤去（SU7）で `e2e/`（WebView2 前提の Playwright）が丸ごと基盤を失う。egui 経路の自動回帰手段は手動 smoke の域を出ていない。**#567（WDIO embedded 移行）は flip 後に無意味化しうる投資**のため、着手前に flip との順序を決める（SU7 受け入れに後継方針の決定を含めた）。
- **並行性の再導入**: SU4 の非同期アイコンバッチと SU5 の起動 async 化（#631）は、SU3 が同期モデルで消した並行性を局所的に戻す。folder 展開で確立した per-nav thread + drain 最新 token パターンを踏襲し、supersede/single-flight 機構の全面復活はさせない。

## 進め方

本ロードマップは Phase 2 の分解であり、実装計画ではない。**各 SU が独立の spec → plan → 実装サイクル**を持つ。次は **#634（G-SYNC 実測）を実行**し、その結果（同期のまま確定 / `spawn_blocking` 化）を踏まえて **SU3.5（tool-selection）を brainstorm** して spec 化する。SU 間で同一ファイルに触れる並行作業は境界を確認してから（ルート `CLAUDE.md`「サブエージェント委譲と worktree」）。

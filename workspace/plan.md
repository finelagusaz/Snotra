# plan — #1336 src-tauri/CLAUDE.md を、テストにも型にも載らない規則だけに絞る

## 目的と受け入れ条件

`src-tauri/CLAUDE.md` から、テスト・型・コード側の doc が既に持つ条項を消し、分類 4（気づきにくく・高くつき・テスト不能・特定箇所に紐づかない）とモジュール索引だけを残す。条項ごとの分類は `workspace/research.md`「振り分け表（段 1 の成果物）」が正本（裁定反映済み）。

受け入れ条件（issue 本文）:

- PR 本文に削除台帳（消した条項ごとに「移した先」か「消してよい理由」を 1 行）
- PR 本文に前後の字数（`LC_ALL=C.UTF-8 wc -m`。前 29,849）・太字数（前 `**` 232 個）・分類ごとの件数（4 の残数を明記）
- `npm run governance:check` が通り、本書を指していた生きた参照で行き先を失ったものが無い
- `cargo fmt --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` が通る

## 裁定（2026-11-01・ユーザー）

1. I5 → `indexing` / `index_build_started` を private 化し型で守る
2. R4 → 消す
3. X5 / X9 → コード側の doc へ移す
4. T1 → 本書に残す（4）。`SnotraTraceInvariants.psm1:8` のずれた指しは本節へ直す
5. P7 → 4 へ寄せる（`#[must_use]` 節に統合）
6. ほかの行は表のとおり

## 書き直した本書の構成（残すもの）

見出しの字面は、外から正準形で指されているものは**逐語で残す**。

- `# src-tauri` — 1 行の紹介（`[lib]` 無しの注意は消す・A1）
- `## モジュール構成` — ファイル名の索引（G-module-index）。各行は「責務は `//!`」に揃え、規範の散文を外す。`commands/`・`platform/` も `//!` を正本にして例外宣言（9 行目）を消す。`heap_trace.rs:23` が指す「モジュール索引行」は残る
- `## 処置を返す純粋核の強制（#934）` — P1〜P5 を縮約（型段/メソッド段の配置・参照/`impl Trait`/ジェネリックで包むときはメソッド段も・メッセージで失うものを名指す・対象外 2 種の述語・`let _ =` は今も通る・`double_must_use` を当てにしない）。関数名の列挙と probe の経緯は削る
- `## イベント駆動 wake の不変条件（#532 SU5） — フレームの paint より後や worker で UI 状態を変えたら \`ctx.request_repaint()\` を撃つ` — E1・E2 に D1・D3（armed 期限の再要求・時間経過で解消する不成立だけ）を統合。`snotra-egui-runtime/CLAUDE.md:24` が消費側の規範としてこの節を指すので統合先として正しい。armed 期限の数え上げ（D2）は消す
- `## main ウィンドウへ egui コンテナを足すときは visuals を自分で渡す` — V7（新設見出し・内容は既存）
- `## \`app.listen\` のコールバックは emit 元のスレッドで同期実行される` — N2（新設見出し・内容は既存）
- `## trace の presence 検査は状態の検査ではない（#671 PR A′）` — T1 そのまま
- `## ウィンドウ生成の制約` — G1・G3（setup もイベントループの中）・G4。帰結 1（manage 順）は `main.rs:295-310` が持つので 1 句で指す

（`###` を `##` へ上げる。3b 測定: `src-tauri/src` の全 basename は索引部分に既に出ており、`## モジュール構成` の範囲が縮んでも G-module-index は動かない——段 2-5 で `governance:check` が確かめる）

## 変更ファイルと対象シンボル

### 型で守る（I5）

- `src-tauri/src/state.rs` — `indexing` / `index_build_started` を private へ。読み口 `pub fn is_indexing(&self) -> bool`・`pub fn is_index_build_started(&self) -> bool`（`SeqCst` で読む）。フィールド doc に「書くのは `try_begin_index_build` / `finish_index_build` / `AppState::new` だけ」（M1 の「コヒーレンシ判断をこの 2 つから導かない」も同所）
- 呼び出し点 5: `commands/system.rs:13`・`commands/window.rs:168,169,180`・`egui_shell/window_coordinator.rs:131`（最後は `Relaxed` → 読み口経由で `SeqCst`。読みを強める向きなので挙動は変わらない）
- フィールド doc に「2 フラグは別物——`index_build_started` は CAS 専用、`indexing` は first-run でビルドスレッド不在でも true になる UI 表示用」も 1 句（plan-review 軽微 2）
- テストは既存の `state.rs` の 5 本（モジュール内なので private のまま読める）

### 3+（コード側へ書き足す）

| 条項 | 移設先 |
|---|---|
| M3 命令 | `indexing::build_index_from_material` doc に「PATH マージの規則を変えるときは `main.rs` の起動経路も」。`main.rs` の PATH マージ箇所から同関数への指し |
| M8 / C1–C3 | `commands/mod.rs` に `//!` 新設: 責務（egui view・トレイが共有する core 関数）と返り値の 3 系統・`bool` 返しを新規に使わない・`ERR_<状態>` 命名と `ERR_INDEXING_IN_PROGRESS` の共有 |
| M8 | `platform/mod.rs` に `//!` 新設: Win32 メッセージループスレッド + トレイ + ホットキー + wndproc の責務 |
| M9 | `platform/hotkey.rs` に `//!` 新設: core の `ParsedHotkey` だけを変換し永続文字列を再解釈しない・登録と smoke の `vks` は同じ変換結果から |
| M13 / M14 | `window_coordinator.rs` `//!` の「main のサイズは 2 か所」段落へ、CLAUDE.md の幾何 1（同じ高さ・`status_row_present` / `main_window_height` の共有・ずれは 1 フレームのスナップ）と幾何 2 の 3 箇所の一覧を引き取る。「正本は CLAUDE.md」の指しを消す |
| W1 射程 | `EguiShellState` の `main_waker` フィールド doc: private ゆえ egui_shell 部分木の外からは綴れない・`EguiShellHandles` 側は `pub(crate)` で setup では書ける |
| R2 | `AppState::read_config` doc に「射程は読みだけ——書き込みは `update_config` の 1 本で engine lock の内側（`snotra-core/CLAUDE.md`「engine.rs のロック最小化パターン」）」 |
| X5 | `results_window.rs` `//!` に層の規則（「results の生ハンドルで `.hide()` は黙って no-op」は `results_window.rs:12-16` に既にある＝3=）: 1 ウィンドウの show/hide/topmost は全部 tao か全部 raw・main=tao / results=raw・「main の show だけ raw」は禁止（`set_always_on_top` が main を消す） |
| X9 | `window_coordinator::hide_egui_main` doc: 5 関数の閉包は表現不能化ではない・`Manager` から main を `.hide()` すると効くが `main_visible` が更新されず results が取り残される・main を隠す経路は `hide_egui_main` を通す |
| X8 | `window_coordinator.rs` `//!`: 可視性を変える新しい操作にも `&EventLoopProof` を要求する（型は付け忘れ・引数の除去を捕まえない——変異で確かめたのは「別スレッドから呼べない」だけ。plan-review 要対処 5）。「外してはならない」は `show_egui_main` doc に既にある |
| N4 | `platform/wndproc.rs` `platform_default_wnd_proc` doc に「メッセージハンドラを削る前に、届く全経路を列挙する（`WM_CONTEXTMENU` が 2 経路の実例）」 |
| F2 / F4 | `font_stack.rs` `//!`: softbuffer の `fill_mesh` はカバレッジ AA を持たず分数 px の差を段差にする・フォント登録を触ったら目視スモーク（`docs/build-commands.md` カテゴリ D）を省かない |
| X17 | `commands/launch.rs` `expand_env` の clamp 行に理由（2 呼び出し間で値が伸びうる・release は `panic="abort"`） |
| X19 | `launch_item_core` に COM STA が要る理由（フォルダ・文書はシェル拡張経由）を 1 句 |
| X21 | `platform/mod.rs` の `SnotraPlatformWindow` 生成箇所: platform スレッド所有ゆえ、イベントループ側から宛てる `SendMessageTimeoutW(WM_NULL)` の同期待ちは有効（tao のウィンドウ宛ては自スレッドで no-op） |
| S3 | `working_set.rs` `//!` に「削るのは working set であって commit ではない・HANDLE は RAII で閉じる」 |

### 指しの付け替え・偽の訂正

| 場所 | 現在 | 付け替え先 |
|---|---|---|
| `state.rs:37`・`layout.rs:453`・`results_window.rs:28,122`・`SPEC.md:627`・`hide_request.rs:5-6`（「ウィンドウ生成の制約」の後半「可視性を変える操作は…」） | 「可視性を変える操作は…」の bullet 群 | `window_coordinator::hide_egui_main` の doc（`hide_request.rs` は「ウィンドウ生成の制約」の指しを残し、後半だけ付け替え） |
| `state.rs:101`・`activation.rs:348`・`search_flow.rs:44`・`window_coordinator.rs:192`・`docs/architecture.md:247` | 「モジュール構成」の #1032 条項（当該条項） | `AppState::read_config` の doc（`state.rs:101` は自身なので指しを消す） |
| `view.rs:972,1239`・`layout.rs:394`・`egui_shell/mod.rs:21`・`window_coordinator.rs:26` | 「モジュール構成」の `window_coordinator.rs` の項 | `window_coordinator.rs` の `//!` |
| `view.rs:1205` | 「モジュール構成」の trace 規範 | 本書「trace の presence 検査は状態の検査ではない」 |
| `results_window.rs:151` | 「Win32 / Tauri 注意事項」の規約 | `ResultsWindow` の `//!`（X5） |
| `results_window.rs:337` | 「この種の race を lock で囲んではならない」 | 指しを消す（同所が内容を持つ） |
| `working_set.rs:5`・`window_coordinator.rs:530,545`・`PERFORMANCE.md:108` | 「working set の能動回収」 | `working_set.rs` の `//!`（自身は指しを消す。`window_coordinator.rs:530` の「hide 前後どちらでも無害」は `//!` に無いので、530 は指しを消して本文に任せる） |
| `config_watcher.rs:8` | 「CLAUDE.md を正とする」 | 本体コメント（`apply_config_change`）を正本と書き換え |
| `config_watcher.rs:23` | `egui_shell::visual::native_brush_color`（偽） | `window_coordinator::native_brush_color` |
| `commands/system.rs:19` | 「共有 core 関数の返り値契約」 | `commands/mod.rs` の `//!` |
| `lifecycle.rs:35`・`search_state.rs:129`・`notify.rs:84`・`window_coordinator.rs:848`・`hide_request.rs:5`・`window_coordinator.rs:412,476,557`・`main.rs:302,616` | 残る見出し | そのまま（字面を残す） |
| `main.rs:342,413` | `(src-tauri/CLAUDE.md)`（該当記述なし） | `docs/architecture.md`「起動シーケンスと初期化順序」 |
| `.claude/rules/src-tauri.md` | 「実装パターン」「Win32 / Tauri 注意事項」×3 | index-build フラグ行は「型が守る（private）」で消す。`windows` crate 行は消す（X3=5）。`ShellExecuteW` 行 → `launch_item_core`。`PlatformBridge` 行は「個別の理由は…」の指しを消す（直呼び箇所ごとの理由 doc の実在は未確認ゆえ、指し先を作らない）。「ウィンドウ生成の制約」行は残す |
| `.claude/agents/code-reviewer.md:68` | 「モジュール構成」（偽） | `src-tauri/src/events.rs` の `//!` |
| `.claude/agents/code-reviewer.md:89` | 「共有 core 関数の返り値契約」 | `src-tauri/src/commands/mod.rs` の `//!` |
| `.claude/agents/code-reviewer.md:124` | 「テーマ色・font・行高の読みは 1 フレーム 1 回」 | `src-tauri/src/egui_shell/visual.rs` の `//!` |
| `SPEC.md:767` | 「共有 core 関数の返り値契約」 | `src-tauri/src/commands/mod.rs` の `//!` |
| `PERFORMANCE.md:87` | 「working set の能動回収」 | `src-tauri/src/working_set.rs` の `//!` |
| `scripts/lib/SnotraTraceInvariants.psm1:8` | 「Win32 / Tauri 注意事項」（ずれ） | 本書「trace の presence 検査は状態の検査ではない」 |
| そのまま: `race-check` SKILL・`snotra-egui-runtime/CLAUDE.md:24`・`build-commands.md:92`・`smoke-egui.ps1:701`・`visual-check-colors.ps1:32`・`G-heading-refs.mjs:16`・`architecture.md:61`・`development-principles.md:36`・`comment-guidelines.md:174` | 残る見出し / パスのみ | 変更なし |
| ADR・`docs/superpowers/`・`docs/design/` | 消えた見出し | 直さない（凍結された歴史・PR 本文に 1 行） |

### 触るファイルの全パス（prose:sweep の照合用）

`src-tauri/CLAUDE.md`・`src-tauri/src/state.rs`・`src-tauri/src/indexing.rs`・`src-tauri/src/config_watcher.rs`・`src-tauri/src/main.rs`・`src-tauri/src/working_set.rs`・`src-tauri/src/commands/mod.rs`・`src-tauri/src/commands/system.rs`・`src-tauri/src/commands/window.rs`・`src-tauri/src/commands/launch.rs`・`src-tauri/src/platform/mod.rs`・`src-tauri/src/platform/hotkey.rs`・`src-tauri/src/platform/wndproc.rs`・`src-tauri/src/egui_shell/mod.rs`・`src-tauri/src/egui_shell/view.rs`・`src-tauri/src/egui_shell/layout.rs`・`src-tauri/src/egui_shell/results_window.rs`・`src-tauri/src/egui_shell/window_coordinator.rs`・`src-tauri/src/egui_shell/font_stack.rs`・`src-tauri/src/egui_shell/launcher_controller/activation.rs`・`src-tauri/src/egui_shell/launcher_controller/search_flow.rs`・`.claude/rules/src-tauri.md`・`.claude/agents/code-reviewer.md`・`SPEC.md`・`PERFORMANCE.md`・`scripts/lib/SnotraTraceInvariants.psm1`

`src-tauri/src/egui_shell/launcher_controller/hide_request.rs`・`docs/architecture.md`（:247 のみ）も触る。

**触らないが sweep に出るもの（裁定済み）**: `src-tauri/src/egui_shell/lifecycle.rs`・`src-tauri/src/egui_shell/search_state.rs`・`src-tauri/src/egui_shell/notify.rs`（残る見出しを指す）／`docs/development-principles.md:76`（見出し語「config の読みは read_config を通す」を書式の例示に使うだけで参照ではない。3 本目の範囲）／`src-tauri/src/heap_trace.rs:23`（「モジュール索引行」は残る）／`src-tauri/src/indexing.rs:99`（`index_build_started=true のまま wedge` は private 化後も真）／`.claude/skills/race-check/SKILL.md`・`snotra-egui-runtime/CLAUDE.md`・`docs/build-commands.md`・`scripts/smoke-egui.ps1`・`scripts/visual-check-colors.ps1`（残る見出しを指す）／`docs/comment-guidelines.md`・`docs/development-principles.md`・`docs/architecture.md:61`（パスだけ）／`docs/design/2026-05-31-coherence-staleset.md`・`docs/adr/`・`docs/superpowers/`（凍結された歴史）

## 削除台帳に必ず書く行（plan-review 軽微）

- R4: `/history` 枝（`recent_history`）と `record_folder_expansion` が engine 錠越しに待つ事実は文書から消える（裁定 2）
- X4: 一般則「宣言的属性で挙動を代替する前に全分岐を確かめる」は消え、機序（`MARKER_DONT_FOCUS`）だけが `results_window.rs:108-111` に残る
- M14 末尾（`read_frame_geom` / `logical_to_phys`・読み戻し経路の消滅）は `ADR-show-path-derives-bar-rect` と `show_egui_main` 本体コメントが持つ → 5
- ADR・`docs/superpowers/`・`docs/design/` が消えた見出しを指しても直さない（凍結された歴史）

## 実装順序

1. I5 の private 化 → `cargo test -p snotra`（Red は不要: 型の変更。委譲先が「外から `store` する変異で E0616」を確かめる）
2. 3+ の書き足し
3. `.rs` の指しの付け替え・偽の訂正
4. 本書の書き直し
5. 外部文書の付け替え
6. 検証と数え上げ

## 不変条件と異常系

- 見出しを消すと、正準形で指す `.rs` / `.md` が G-heading-refs で赤になる → 付け替え漏れの検出器。**パス無し・散文形は載らない** → 上の表を 1 行ずつ潰し、最後に `git grep -n 'CLAUDE.md' -- src-tauri/src .claude scripts SPEC.md PERFORMANCE.md docs ':!docs/adr' ':!docs/superpowers'` と消えた見出し名の `git grep` で取りこぼしを数える
- `## モジュール構成` の範囲が縮む → G-module-index が赤なら索引に basename を足す
- 組み直しで命題が落ちる → `.claude/rules/governance-docs.md` の作法: 旧版と新版のバッククォート語・issue 番号・ADR 名を集合差で突き合わせ、落ちたものは削除台帳か移設先のどちらかに載っていることを確かめる
- I5: 読み口の追加で `Ordering` が変わる箇所（`window_coordinator.rs:131` の `Relaxed`）→ 強める向きのみ

## テスト方針と検証コマンド

- `docs/build-commands.md`「変更後の検証チェックリスト」カテゴリ A（src-tauri）・`cargo doc` 行・`npm run governance:check`
- 変異（委譲先が複製で）: I5 — `state.rs` の外から `state.indexing.store(..)` で E0616
- 挙動を変えない（コメントと可視性のみ）→ smoke 不要。ただし `cargo test -p snotra` の件数が前後で同じことを確かめる

## SPEC.md・関連文書

- SPEC.md は参照の字面のみ（`SPEC.md:767`）。挙動の変更なし

## フェーズ別作業項目

- [ ] 段 2-1: I5 の private 化と呼び出し点 5 の書き換え
- [ ] 段 2-2: 3+ の書き足し（上の表の全行）
- [ ] 段 2-3: `.rs` の指しの付け替えと偽の訂正（上の表の全行）
- [ ] 段 2-4: 本書の書き直し（「書き直した本書の構成」）
- [ ] 段 2-5: 外部文書の付け替え（rules・code-reviewer・SPEC・PERFORMANCE・psm1）
- [ ] 段 2-6: 検証（カテゴリ A・`cargo doc`・`governance:check`・集合差・取りこぼしの `git grep`）
- [ ] 段 2-7: 削除台帳・字数・太字数・分類件数を数える（PR 本文の材料）

## 未確定（実装前に潰す）

- [x] 振り分け表の裁定（段 1）— 2026-11-01 ユーザー裁定（上の「裁定」節）
- [x] I5 — private 化（裁定 1）。書き手 0・読み手 5 を grep 済み
- [x] R4 / X5 / X9 / T1 / P7 — 裁定済み
- [x] `1?` の変異測定 — 10/10 CAUGHT（research.md「変異測定」）
- [x] 敵対的調査（3b）の所見の採否 — research.md「敵対的調査（3b）の採否」

## 人間レビュー

- [x] 承認済み — 2026-11-01 / 問い: "計画を承認いただけますか。" / 回答: "承認"

## plan-review 結果

- リスク: 高（ガバナンス文書の圧縮・`--deep` 明示）
- レビュー方式: 計画準拠レビュー 1 体（Step 2——issue に無い人間の裁定が前提にあるため Step 2b ではなく Step 2）
- エージェント数: 1（出力 `workspace/plan-review-refs-propositions.md`）

### 要対処
- SPEC.md:627 の指しが表に無い — 計画の修正 — Grep で SPEC.md:627 に `src-tauri/CLAUDE.md` を確認
- PERFORMANCE.md:108 の指しが表に無い — 計画の修正 — 同 PERFORMANCE.md:108
- docs/architecture.md:247 の「当該条項」 — 計画の修正 — 同 architecture.md:247
- hide_request.rs:5-6 の後半（「可視性を変える操作は…」） — 計画の修正 — `sed -n 4,7p` で確認
- X8 の「新しい可視性操作にも証人を要求」が落ちる — 計画の修正（`window_coordinator.rs` `//!` へ 3+） — 変異は別スレッド呼び出しだけを測った（research.md「変異測定」）
- X5 の表現不能化側の所在を明記 — 計画の修正 — `results_window.rs:12-16`

### 軽微
- R4・X4・M14 末尾を削除台帳に明記（反映）／I5 のフィールド doc に 2 フラグの別を 1 句（反映）／`window_coordinator.rs:530` は指しを消す（反映）／`development-principles.md:76` は触らない側に一覧（反映）／rules の PlatformBridge 行は指しを消す（反映）

### 未検証
- SPEC.md:627 のような bullet リード形の参照を G-heading-refs が拾うか — 段 2-6 の `governance:check` と取りこぼしの `git grep` で確かめる（計画の検証項目に含まれる）

### 判断
- 実装着手: 可

## セルフレビュー

- リスク: 高
- plan-review: 独立レビュー 1 体（Step 2）
- エージェント数: 1（3b を含めると 2）
- 要対処: 6 件——指しの漏れ 4・X8 の命題・X5 の所在。すべて計画へ反映
- 未検証: G-heading-refs が bullet リード形を拾うか（段 2-6 で測る）
- 散文の全件列挙: 語 8 / 総件数 228 行 / ★未掲載 69（全パス一覧を足した。残る ★ は ADR・superpowers・design の凍結された歴史と、触らない側に裁定した行）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2 / 設計変更 0 / 主の見落とし 6

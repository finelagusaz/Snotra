# plan-review Step 2b: issue #697 独立再導出

- 日付: 2026-07-26
- 対象 issue: #697（#671/#673 サイクルの残余: hidden 中の paint 抑止機構の実測 + トートロジーテスト + 決定 5 の記録欠落）
- 導出者: 独立レビュアー（`workspace/plan.md` / `research.md` / `frame-scheduling-design.md` は読んでいない。根拠は issue 本文・コメントの実測データ・コードのみ）

## 1. 要件の理解（3 項目の WHAT）

### 項目 1: 「hidden 中は update() が走らない」の抑止機構の同定（実測）

WHAT: #532 SU5 以来の設計前提でありながら「何が抑止しているのか」を測っていない不変条件について、抑止点を **送信側（scheduler worker が proxy へ撃ったか）/ 受信側（`RedrawRequested` が届いたか）** の 2 計器で切り分ける。issue コメントの #628 副産物実測により「受信側には来ていない（`render()` は呼ばれていない）」までは確定済み。残る二択は:

- (A) worker は `proxy.send_event(Message::Window(id, RequestRedraw))` を撃ったが、tao/OS の経路が hidden HWND 宛を落とした
- (B) そもそも撃たれていない（deadline 管理側で消えた）

### 項目 2: `hidden_window_is_not_painted` の処分（トートロジーテスト）

WHAT: `snotra-egui-runtime/src/runtime.rs` のテスト `hidden_window_is_not_painted` はテスト内ローカル定義の `fn should_render(visible: bool) -> bool { visible }`（恒等関数）を検査しており、実 `render()` の早期 return を何も守っていない。選択肢は **「削る」か「接地したコメント付きで残す」の 2 つだけ**（実 `render()` を検査する形は不可能——`EguiWindow::new` が実 `tauri::Window` + 実 HWND 上の `ImeBridge` を要求し、crate は dev-dependencies ゼロ・mock runtime 無し）。**どちらを採るかは項目 1 の測定結果を見てから決める**（順序依存）。あわせて `render()` 内の到達可能性注記の「（本ブランチ b9a9caf）」というマージ後に宛先を失う参照を PR #677 へ言い換える。

### 項目 3: spec 決定 5 が要求した記録の欠落（`src-tauri/`・独立着手可）

WHAT: `drive_results_window` 末尾の無条件 `wake_results`（level-triggered・毎フレーム）に「削ると壊れる理由」のコメントが無い。理由（実装で今も成立していることを確認した）:

1. `register_config_wake_listeners` は `CONFIG_APPLIED` / `INDEXING_STARTED` / `INDEXING_COMPLETE` で `wake_main` **のみ**を呼ぶ——results は config 系イベントを一切 listen していない
2. results 可視中に visual-only の config 変更（`font_size` / `row_padding` / 各色 / `show_icons`）が入っても `RowsSnapshot`（rows / selected / generation / settled）は不変ゆえ、差分 wake（snapshot publish 側の edge-trigger）も発火しない
3. したがって **results が新しい visual 値を描く唯一の経路がこの無条件 wake である**。「毎フレーム wake は無駄」という一見正しい最適化で消すと静かに壊れる

これを呼び出し点に 1〜4 行で記録する。

## 2. 必要な変更集合（ファイル + シンボル + 1 行説明）

### 項目 1（計器 2 つ + 計器正本の更新）

| # | ファイル | シンボル | 変更 |
|---|---|---|---|
| 1-1 | `snotra-egui-runtime/src/repaint.rs` | `RepaintScheduler::new` の worker クロージャ、`None`（deadline 満期）arm の `proxy.send_event(...)` **直前** | env ゲート（例 `SNOTRA_EGUI_WAKE_TRACE`）の `eprintln!` 1 行（`window_id` を含める）。#628 の計器と同じ流儀（未設定なら var_os 判定のみでコスト 0） |
| 1-2 | `snotra-egui-runtime/src/runtime.rs` | `RuntimePlugin::on_event` の `Event::RedrawRequested(window_id)` arm **冒頭**（`context.window_id_map.get` **より前**） | 同じ env ゲートの `eprintln!` 1 行（tao 側 `window_id` を含める）。lookup より前に置く（未知窓宛も数えるため） |
| 1-3 | `PERFORMANCE.md` | 「計測と受け入れ基準」の egui 計器一覧（**このリストが計器の正本**と自己宣言している） | 新 env 変数を 1 行追加（出す項目・目的・#697） |

計器は測定後も削除せず env ゲート付きで残す（#628 `SNOTRA_EGUI_REPAINT_TRACE` / `SNOTRA_EGUI_PAINT_TRACE` と同じ扱い）。測定結果の記録先は issue #697 コメント（アイドル基準値は変わらないため `PERFORMANCE.md` の基準値節は不変）。

### 項目 1 の測定結果を文書へ反映する変更（測定後・機構が同定できた場合）

| # | ファイル | シンボル | 変更 |
|---|---|---|---|
| 1-4 | `snotra-egui-runtime/src/runtime.rs` | `EguiWindow::render` 冒頭の到達可能性注記 | 「未測定（OS/tao が…と推定）」を実測結果（(A) or (B)）+ 測定ログの所在へ書き換え |
| 1-5 | `src-tauri/src/egui_shell/mod.rs` | `wake_main` の doc | 「その抑止は wake 経路ではなく OS/tao 層にあると推測されており未測定」の限定並記を実測結果へ書き換え |
| 1-6 | `src-tauri/src/egui_shell/layout.rs` | `results_should_show` の doc | 「機構は未同定・未測定・spec §7-2 が…」を実測済み（#697）へ書き換え（「命題に依存しない」という主張自体は不変） |
| 1-7 | `src-tauri/CLAUDE.md` | 「イベント駆動 wake の不変条件（#532 SU5）」節の「**hidden 中は `update()` が走らない**（実測・SU5 要石）」 | 同定した機構と #697 を括弧内に追記（挙動主張は不変・小変更） |
| 1-8 | `docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md` | §7 残余 2・残余 3 | errata 1〜2 行ずつ（「#697 で測定済み/処分済み」）。日付付き spec は歴史記録ゆえ本文は書き換えず errata **追記**（646 spec の errata 前例に倣う）。任意だが、§7 の自己目的（「次のサイクルが誤認しないため」）に照らし推奨 |

### 項目 2（測定後に確定）

| # | ファイル | シンボル | 変更 |
|---|---|---|---|
| 2-1 | `snotra-egui-runtime/src/runtime.rs` | `tests::hidden_window_is_not_painted` | **削除を推奨**（恒等関数の検査は何も証明せず、残せば「守られている根拠」として再引用されるリスク——spec §7-3 が警告した失敗様式そのもの。接地した知識は 1-4 の `render()` コメントに置く方が co-location として正しい）。残す場合は「意図的に残した到達不能ガードの述語の形だけを固定する・実挙動の証明ではない・#697」とコメントを書き換える |
| 2-2 | `snotra-egui-runtime/src/runtime.rs` | `render()` 注記の「`RuntimeFrame::hide_window()` 削除（本ブランチ b9a9caf）により」 | 「（PR #677）」へ言い換え（**測定と独立・即着手可**） |

判断基準: (A)（tao/OS 経路で落ちる）でも (B)（送信側で消える）でも、テストが実挙動を守れない事実は変わらないため削除の結論は同じ。測定結果の分岐が効くのは 1-4 のコメントに何を書くか（「受け口として残す `visible` ガード」の必要性の説明）だけである。

### 項目 3（独立着手可）

| # | ファイル | シンボル | 変更 |
|---|---|---|---|
| 3-1 | `src-tauri/src/egui_shell/view.rs` | `drive_results_window` 末尾の `crate::egui_shell::wake_results(&self.app_handle);`（現行 :857。issue 記載の :850 は #675 クランプ追加で下方シフト） | 直前に 1〜4 行のコメント: 「results は config 系イベントを listen せず（`register_config_wake_listeners` は `wake_main` のみ）、visual-only 変更では `RowsSnapshot` 不変ゆえ差分 wake も発火しない。results が新しい config 値を描く唯一の経路がこの無条件 wake である。削ると visual 反映が静かに壊れる（spec 決定 5・#697）」 |
| 3-2（任意） | `src-tauri/src/egui_shell/mod.rs` | `wake_results` の doc | 「level-triggered」ラベルの後に「削ると壊れる理由は `drive_results_window` 側コメント」の 1 行ポインタ（理由の本文は 3-1 に一元化・写しを作らない） |

## 3. 測定手順（項目 1）

- **前提**: release ビルド。`predicted_dt` は #628/PR #709 で既定 0 に変更済み——**この変更後のコードで測る**ことを結果の前提として明記する
- **env**: `SNOTRA_EGUI_WAKE_TRACE=1`（新設・送受信 2 計器）+ `SNOTRA_EGUI_REPAINT_TRACE=1`（フレーム到着の照合）+ `SNOTRA_TRACE=1`（hide の trace 時刻同期）。stderr をファイルへ捕捉
- **刺激**:
  1. 可視・フォーカスありでアイドル（TextEdit のキャレット点滅が毎パス `request_repaint_after(≤0.5s)` を撃つ——hide の瞬間に満期前 deadline が scheduler に必ず 1 つ載る「自然な刺激」）
  2. Alt+Q（実打鍵・人間が必要。ホットキーは `GetAsyncKeyState` 依存で自動化不能）で hide し 30 秒放置
  3. **強い刺激**: hidden のまま `config.toml` を書き換えて `config-applied` を発火させる（`register_config_wake_listeners` は可視性を見ずに `wake_main` を撃つため送信要求が確実に発生する）。**visual-only キー（`font_size` 等）を変える**——`IndexInputs` に効くキーを触ると `indexing-*` イベントが混ざりノイズになる。watcher の debounce は 100ms ゆえ書き換えは 1 回でよい
- **観測点**: `egui_hide:done`（SNOTRA_TRACE）以降の観測時間内の、(i) 送信計器行数、(ii) 受信計器行数、(iii) `SNOTRA_EGUI_REPAINT` 行数（0 のはず・既測の再確認）
- **判定基準**:
  - 送信 ≥1 かつ 受信 0 → **(A) 確定**（tao/OS 経路が hidden HWND 宛を落とす。キャレット deadline と config 刺激の両方で送信が出れば確度が上がる）
  - 送信 0 → **(B) 確定**（deadline 管理側で消えている——`repaint.rs` worker 内のさらなる切り分けへ進む）
  - 送信 ≥1 かつ 受信 ≥1（かつ REPAINT も出る）→ **不変条件そのものが偽**。文書反映（1-4〜1-8）には進まず、いったん停止して issue へ報告する

## 4. 間接参照の列挙（「hidden 中は update() が走らない」を引用している全箇所と処分判断）

概念で分類する。**(a) 機構を主張している箇所**（「未測定」「OS/tao と推測」を並記——測定後に書き換え対象）、**(b) 挙動だけを引用している箇所**（不変条件は測定後も真のまま——書き換え不要）、**(c) 日付付き歴史文書**（凍結——書き換えない）。

### (a) 機構に言及・測定結果で書き換えるべき（5 箇所）

| 箇所 | 現状の記述 | 判断 |
|---|---|---|
| `snotra-egui-runtime/src/runtime.rs` `render()` 注記 | 「どの機構に依るか未測定（OS/tao が…と推定）」+「本ブランチ b9a9caf」 | **書き換える**（1-4・2-2） |
| `src-tauri/src/egui_shell/mod.rs` `wake_main` doc | 「抑止は wake 経路ではなく OS/tao 層にあると推測されており未測定」 | **書き換える**（1-5） |
| `src-tauri/src/egui_shell/layout.rs` `results_should_show` doc | 「機構は未同定・未測定・spec §7-2」 | **書き換える**（1-6）。「命題に依存しない」は保持 |
| `src-tauri/CLAUDE.md` イベント駆動 wake 節 | 「hidden 中は `update()` が走らない（実測・SU5 要石）」 | **追記推奨**（1-7）。挙動主張は真のまま・機構の出典 #697 を足すだけ |
| spec 2026-07-25 §7 残余 2・3 | 「未同定・未測定」「恒真テストである」 | **errata 追記を推奨**（1-8）。本文改変はしない（歴史記録） |

### (b) 挙動だけを引用・書き換え不要（不変条件は測定後も成立）

| 箇所 | 引用の形 |
|---|---|
| `SPEC.md`（フレームスケジューリング節「非表示中はフレームが走らない」） | 挙動仕様。機構主張なし |
| `docs/architecture.md`（「hidden 窓は `update()` が走らないため自分では show できない」） | 設計根拠としての挙動引用 |
| `src-tauri/src/egui_shell/results_view.rs` `//!`（driver は main 側） | 同上 |
| `src-tauri/src/egui_shell/mod.rs` `create()` 内コメント（「hidden 窓は update() が走らないため自分では show できない」） | 同上 |
| `src-tauri/src/egui_shell/mod.rs` `EguiShellState` フィールド doc（「wake は可視中のみ意味を持つ」） | 同上 |
| `src-tauri/src/egui_shell/mod.rs` `wake_results` doc（「hidden 中の results は描かれないため事前 wake は無意味」） | 同上（3-2 の任意ポインタ追記のみ） |
| `src-tauri/src/egui_shell/mod.rs` `register_initial_hotkey_failure_listener` doc（「ここで起こさないと…通知が永遠に描かれない」・統合禁止の非対称根拠） | 同上 |
| `src-tauri/src/egui_shell/view.rs` タイムアウト系コメント（「可視中のみ有効…reset-on-show が backstop」）と `drive_results_window` 呼び出し前コメント（「毎フレーム走る main が駆動」） | 同上 |
| `src-tauri/src/egui_shell/results_window.rs`（show 述語ゲートへの参照） | 同上 |
| `.claude/skills/race-check/SKILL.md`（「hide / show を跨ぐか」項） | 正本（`src-tauri/CLAUDE.md`）へのポインタ。**変更不要**。かつスキル変更は合意が要る（最重要ルール 2）——触らない |
| `docs/adr/0003`（「事実…はスキルに写さず `src-tauri/CLAUDE.md` の節名で参照」） | 参照方式の記録。変更不要 |

### (c) 日付付き歴史文書・凍結（書き換えない）

`docs/superpowers/specs/2026-07-24-su5-updater-notification-design.md`（hidden 中の drain・要石スモーク）/ `2026-07-24-su6-config-glue-design.md`（wake 空振り無害・SU5 実測）/ `2026-07-24-646-two-window-ui-design.md`（errata・SU5 要石）/ `2026-07-21-su1-softbuffer-runtime-design.md`（不変条件⑥）/ `docs/superpowers/plans/` 配下の各計画（pr-a・pr-c・pr-d・su5・su6・su6.5・646-pr2）。いずれも当時の判断記録であり書き換えない（唯一の例外が (a) の 2026-07-25 spec への errata **追記**）。

**同名・別概念の注意**: grep「走らない」は `.claude/hooks/`（「import しただけでは main() が走らない」）・`.claude/rules/governance-docs.md`（「PostToolUse hook 検査は走らない」）・`src-tauri/CLAUDE.md` setup 節（「egui フレームは 1 枚も走らない」——setup 中のポンプ停止の話で別概念）にも当たる。これらは本不変条件と無関係——変更対象に含めない。

## 5. 落とし穴・注意点

1. **行番号は既にずれている**: issue 記載の `view.rs:733` / `view.rs:850` / `mod.rs:531-538` / `runtime.rs:306` は現行では `view.rs:857`（無条件 wake）/ `mod.rs:551-563`（`wake_results` doc）/ `runtime.rs:309-316`（注記）。シンボル名で位置決めする（`.claude/rules/src-tauri.md` の #588 規範）
2. **送信側と受信側の window_id は型が違う**: 送信側は `tauri_runtime::window::WindowId`、受信側 arm は tao の `WindowId`。字面は一致しない。切り分けは件数（0 か ≥1 か）で足りるため相関付けは不要だが、ログを読むときに「同じ id が出ない」ことに驚かないこと。窓は main / results の 2 つで scheduler worker も 2 本ある——送信行には必ず id を含める
3. **受信計器は可視中も毎フレーム出る**: 可視アイドル 2fps（`PERFORMANCE.md` 基準値）でログが伸びる。観測は `egui_hide:done` 以降の区間で数える。「trace の presence 検査は状態の検査ではない」（`src-tauri/CLAUDE.md`）——ここでは「区間内に受信が現れない」という不在の観測が判定の主役であり、規範が許す形（区間内に事象が現れないこと）に整合する
4. **`Focused(true)` で `visible` は恒真に戻る**: `render()` の早期 return は現在到達不能なので、「受信したのに REPAINT 行が出ない」という紛れは起きない（受信すれば必ず REPAINT 行が伴う）。受信計器と `SNOTRA_EGUI_REPAINT` 行数の一致はクロスチェックに使える
5. **config 刺激は visual-only キーで**: `IndexInputs::from_config` に効くキー（scan 対象等）を触ると `start_index_build` → `indexing-*` が発火してログが濁る。`font_size` 等の visual キーが安全
6. **計器を足す編集自体が hook を回す**: `*.rs` 編集で clippy + crate テストが走る（沈黙=合格）。`PERFORMANCE.md` / `CLAUDE.md` / spec への追記は hook 検査対象外（沈黙は「何も走らなかった」）——ガバナンス文書に触れるため `npm run governance:check`（カテゴリ F）を手で回す
7. **項目 2 で「実 render() を検査する形」を提案しない**: issue が明示的に選択肢から除外している（dev-dependencies ゼロ・mock runtime 無し・実 HWND 要求）。レビューで「テストを強くする」方向の提案が出たら本 issue の制約を引く
8. **測定は人間との協働が必須**: Alt+Q 実打鍵（`GetAsyncKeyState` 依存で自動化不能）+ 30 秒待ち + hidden 中の config 書き換え。自動 smoke には載らない。実測ログの保存名・保存先を決めてから始める（issue コメントの `628-idle-clean.log` の流儀）。また**検査対象を変更しながら測らない**——計器追加のコミットを固めてから測る
9. **順序依存**: 項目 2 の処分（2-1）と文書反映（1-4〜1-8）は測定後。項目 3（3-1/3-2）・b9a9caf 言い換え（2-2）・計器追加（1-1〜1-3）は測定前に着手可能
10. **「送信 ≥1 かつ 受信 ≥1」への備え**: この結果は不変条件の反証であり、(b) 群の全箇所が誤りになる大事故。判定基準に明記し、その場合は文書反映へ進まず issue へ差し戻す

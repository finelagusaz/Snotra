# 計画 — issue #1268 smoke-egui に Ctrl+英字の打鍵を足す

## 目的と受け入れ条件

`snotra-egui-runtime/src/input.rs` の `on_keyboard_event` が文字を `KeyEvent.text` から取る退行（変異 1）を、`scripts/smoke-egui.ps1` が赤にする。

- AC1: 現行ビルドで `npm run smoke:egui` が exit 0
- AC2: `typed_text_event(pressed, event.text_with_all_modifiers())` を `event.text.as_deref()` 相当へ替えたビルド（変異 1）で exit≠0、かつ赤の理由が新しい断言の failure 文言である（コンパイルエラー・別検査の赤ではない）
- AC3: Ctrl+H の打鍵が届かなかったとき緑にならない（観測なし → 赤）
- 射程外: 変異 2（文字の配送を `admit_key` より前へ）。現行の操作列では通常捕まらない（show 前に hotkey の全キーが解放される。Alt 解放が 350ms を超えて遅れる場合は除く・未測）。捕まえるには #927 の再現形（キーを押したまま focus を移す）が要り、issue が別判断としている

## 設計（research.md「敵対的調査（3b）の反映」B1 で Ctrl+G 案から差し替え）

パスクエリブロックの直後（入力欄は `c:\` の 3 文字）に **Ctrl+H** を注入する。egui 0.36.2 は Windows でも Ctrl+H を 1 字削除に束縛している（`builder.rs:1431`）。

| ビルド / 事象 | `egui_input:changed`（ベースライン以降） | 判定 |
|---|---|---|
| 正常 | 3→2 の 1 件（`text_with_all_modifiers` = `"\x08"` は `is_printable_char` が弾く） | 緑 |
| 変異 1 | 3→3 の 1 件（同じ `on_keyboard_event` が `Key(H)` と `Ime(Commit("h"))` を同じ RawInput に積む → 削除してから `h`） | 赤 |
| Ctrl+H 丸ごと脱落 | なし | 赤（観測なし） |
| Ctrl だけ脱落 | 3→4 | 赤 |

断言: ベースライン以降の `egui_input:changed` が**ちょうど 1 件**で `before_chars=3, after_chars=2`。

## 変更ファイルと対象

1. `scripts/smoke-egui.ps1`
   - パスクエリブロック（`$pathTyped` を作る `if`）の後、WebView2 増分検査の前に新ブロックを置く
   - ゲート: `$failures.Count -eq 0 -and $null -ne $pathTyped`
   - 打鍵前にベースライン `seq` を取る: `$pathTyped.seq`（3 文字の観測そのもの。これより後の変化だけを数える）
   - `Send-SnotraKeyChord -VirtualKeys @(0x11, 0x48)`（VK_CONTROL, H）
   - `Wait-SnotraTraceCondition` で `event -eq 'egui_input:changed' -and [long]seq -gt base` を待つ（`ObserveTimeoutMs`） ——述語は `.GetNewClosure()` を付ける（モジュールスコープで評価されるため・`SnotraSmoke.psm1:686`。既存 :412 と同じ）。不成立なら `$failures += "Ctrl+H ... not observed"`
   - 成立したら短い静定（300ms）の後 `Read-SnotraTraceSnapshot` で区間内の全件を取り、`@()` で包み、1 件かつ 3→2 でなければ `$failures` へ実測（件数・各 before/after）を積む。文言に「`KeyEvent.text` と取り違えると Ctrl 併用で文字が入る（#1268）」を含める
   - ブロック冒頭コメント: 何を捕まえ何を捕まえないか（変異 2 は射程外・Ctrl+A ではなく Ctrl+H を選んだ理由＝打鍵の効果が到達の証拠になる）を書く
   - 隣接の古い記述 :379-380「打鍵から結果までのフレームが予算を超えないことを H6 が判定する」を、判定は無いことが分かる形へ直す（H6 は `SnotraTraceInvariants.psm1` に無い）
   - 末尾の合格メッセージに Ctrl+H を足す
2. `snotra-egui-runtime/src/input.rs` — テストのコメント（:742-743「呼び出し点で `KeyEvent.text` と取り違える形はこのテストに映らない」）に、その形は `scripts/smoke-egui.ps1` の Ctrl+H 打鍵が捕まえることを足す（変異 2 は依然どこにも映らない旨は維持）

SPEC.md: 更新不要（挙動変更なし）。`docs/build-commands.md` の smoke-egui 説明は操作列を網羅していない（パスクエリも載っていない）ので更新不要。

## 不変条件と異常系

- 新ブロックは既存の `$failures` ゲート連鎖に乗り、失敗時は後続の Escape 注入を行わない（既存と同じ）
- 断言は不在ではなく肯定（3→2）で書く——脱落を緑にしない
- Escape 後の `egui_results:hide` 待ちが presence であることは既存の性質で、新ブロックで弱まらない（既存フローでも `z` を消した時点で hide が一度出ている）

## テスト方針と検証コマンド

- `npm run test:powershell`（共有モジュール無変更だが smoke 配管の回帰確認）
- `cargo build --release` 後 `npm run smoke:egui`（AC1）
- `cargo fmt --check` / `cargo clippy`（input.rs はコメントのみ）
- `npm run governance:check`
- 変異注入（AC2・AC3）は `/implement`「3b. 委譲へ渡すもの」の委譲先が worktree で行う: 変異 1 のビルドで赤・理由が新断言であること、および Ctrl+H の注入を外したスクリプトで赤になること

## 作業項目

### Phase 1 — smoke

- [ ] `scripts/smoke-egui.ps1` に Ctrl+H ブロックを追加
- [ ] :379-380 の H6 の記述を直す
- [ ] 合格メッセージを更新
- [ ] 現行ビルドで `npm run smoke:egui` が緑（AC1）

### Phase 2 — 文書

- [ ] `snotra-egui-runtime/src/input.rs` のテストコメントを更新
- [ ] `npm run governance:check`・`cargo fmt --check`・`npm run test:powershell`

## 未確定（実装前に潰す）

（なし——C1〔WM_CHAR が実機で制御文字として届く〕と C5〔変異ビルドで赤〕は AC2 の変異注入が測る。外れたときは設計に戻る）

## 人間レビュー

- [x] 承認済み — 2026-10-10 / 問い: "この計画（Ctrl+H を smoke-egui に足す・変異 2 は射程外）を承認しますか？" / 回答: "OK"

## plan-review 結果

- リスク: 高（CI が走らせるセーフティネットのスクリプトを変更する）
- レビュー方式: 計画準拠レビュー 1 体（観点: 判定表の正しさ／既存シナリオを壊さないか）・成果物 `workspace/plan-review-ctrl-h.md`
- エージェント数: 1（3b の敵対的調査と合わせて 2）

### 要対処

- 述語に `.GetNewClosure()` が要る — 計画の修正（変更ファイル 1）— `SnotraSmoke.psm1:686` が `Where-Object -FilterScript $Predicate` をモジュール内で評価することを再照合

### 軽微

- 変異 1 が積むのは `Event::Text` ではなく `Event::Ime(ImeEvent::Commit)` — 計画・research の記述を訂正（`input.rs:461-464` で再照合）。3→3 の結論は不変
- カーソルが末尾にある前提。外れても赤に倒れる（偽の緑ではない）
- `snotra-egui-runtime/CLAUDE.md` に写しを足さない（`input.rs` のテストコメントと二重になる）——計画どおり

### 未検証

- 変異 1 の実ビルドで 3→3・VK_CONTROL で `modifiers.ctrl` が立つこと — AC2 の変異注入（`/implement` の委譲先）が測る

## セルフレビュー

- 1 issue の要件（Ctrl+英字の打鍵・文字数の断言・変異注入で赤を確かめる）→ Phase 1 と AC2 に対応。変異 2 は issue 自身が別判断としたので射程外に明記
- 2 境界: 脱落（全部 / Ctrl のみ）・同一フレームへの集約・遅れて届く先行変化 → 設計の表と AC3
- 3 新しい状態・リソース: なし（既存プロセス内の打鍵 1 本）
- 4 より単純な形: 後続 Backspace 案・`push_key` trace 案より、効果を持つ Ctrl+H 1 打鍵が単純
- 5 不変条件の検知: 新断言自体が検知器。検知器が効くことは AC2・AC3 の変異注入で確かめる
- 要対処: 1 件（反映済み）

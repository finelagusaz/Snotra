# 計画: #1274 smoke-egui に focus 獲得時の押しっぱなしキーを足し、変異 2 を検知する

## 目的と受け入れ条件

文字の配送（`snotra-egui-runtime/src/input.rs` の `typed_text_event(…)` の push）を、`on_window_event` の `admit_key` による早期 return より前へ動かす退行（変異 2）を、`smoke:egui` が赤にする。

- 正常なコードで `smoke:egui` が緑のまま通る
- 変異 2 を注入すると、**新しい断言の文言で**赤になる（throw や別の断言ではなく）
- 前提が崩れた環境（合成 press が起きない・Z が押されない・hotkey が発火しない）では、沈黙の緑ではなく赤になる
- 既存の断言（シナリオ 1 の show / results / パスクエリ / Ctrl+H / Escape / H1、シナリオ 2）を弱めない
- 失敗しても、押しっぱなしの Z を OS 上に残さない

## 方式

調査（`workspace/research.md`「focus の移し方の候補」）の (C) を採る。文字キーを押したまま hotkey で隠して出し直し、show の `WM_SETFOCUS` で合成 press を作る。(A)（設定ウィンドウ）は CI に `snotra-settings` のビルドと GUI 起動を足す必要があり、(B)（別ウィンドウからの前面化）は補助ウィンドウの所有と前面化の制限を抱える。(C) は新しいプロセスもウィンドウもビルドも要らない。

**置き場所はシナリオ 1 の H1 判定の後**（本体は Escape で隠れている）。それより前に置くと、既存の待ち（1 件でも現れれば成立する `Wait-SnotraTraceEvent`）が新しい事象で満たされ、既存の断言が弱まる。

手順（新しく書く待ちはすべてシナリオ 2 と同じ増分の形 = `Get-SnotraTraceEventCount` + `-MinMatchCount (基準 + 1)`）:

1. hotkey で show する（Z は離れている）。`egui_show:done` を増分で待つ
2. 本体が前面のまま Z を down する（押しっぱなし）。**その show の seq より後に `after_chars = 1` の `egui_input:changed` が出るのを待つ**——Z が OS 上で押されたことの正の証拠であり、文字を hide より前にフレームで消費させる（隠れたウィンドウはフレームが走らない。消費前に hide すると、残った文字が次の show の後に入り、正しいコードでも赤になる）。Z は seed の `zsnotrasmoke.exe` に一致するので results も出るが、無害である（コメントに書く）
3. Z を押したまま hotkey で hide する。`egui_hide:done` を増分で待つ。**この seq を区間の起点にする**
4. Z を押したまま hotkey で show する。`egui_show:done` を増分で待つ（tao が Z の合成 press を作る）
5. 押しっぱなしの Z へもう 1 回 down を送る（非合成の press。`held` に Z があるので落ちるはず）
6. Z を up する
7. 別の英字（X）でふつうの打鍵を 1 回送る
8. 区間（手順 3 の seq より後）の `egui_input:changed` を全件数え、`[0->1]` との完全一致を断言する
9. Escape で hide し、`egui_hide:done` を増分で待つ（smoke を隠れた状態で終える）

区間の起点を手順 4 の show ではなく手順 3 の hide に置くのは、合成の文字が `egui_show:done` より前に出ても区間へ入れるためである（敵対的調査の ⚠️ 所見）。hide から show までに入力欄が変わる経路は無い（`egui_input:changed` の出所は `view.rs:874` の `response.changed()` だけ）。

| 状況 | 区間の変化 | 結果 |
|---|---|---|
| 正常 | `[0->1]`（X だけ） | 緑 |
| 合成 press が起きない | 手順 5 が入り `[0->1, 1->2]` | 赤 |
| 変異 2 | 合成の文字・手順 5・X が入る（件数はフレームへの積まれ方で変わる） | 赤 |
| 打鍵がどれも届かない | 0 件 | 赤 |

## 変更ファイルと対象シンボル

- `scripts/smoke-egui.ps1`
  - シナリオ 1 の H1 判定の後に新しいブロックを足す（try/finally で包み、finally で Z を up する）
  - `:432-433` の「捕まえないもの」のコメントを、新しいブロックが捕まえる旨へ直す
  - `:783` の成功文言に新しい検査を足す
  - `:490-491` の「smoke の実行は最後まで hidden で終わる」は、新しいブロックも Escape で隠して終えるので真のまま。実装時に読み直して確かめる
- `snotra-egui-runtime/src/input.rs`（コメントのみ）
  - `:410` の「`admit_key` の後に置くこと」に、崩すと smoke-egui の押しっぱなしの打鍵が赤にする旨を足す（#1268 が `:742-744` のテスト doc に足したのと同じ形）
- 触らない: `SPEC.md`（挙動は変えない・検査の追加のみ）、`snotra-egui-runtime/CLAUDE.md`（「文字の配送は `admit_key` の後に置く」は規則であって検知器の有無を書いていない）、`docs/build-commands.md`（smoke の検査項目を列挙する箇所は Ctrl+H も載せておらず、写しを増やさない）

## 不変条件と異常系

- 押しっぱなしの Z は、どの経路で抜けても finally で up する（throw・`$failures` の蓄積・正常）
- hotkey が発火しない・show/hide が観測できない → `$failures` に積む（沈黙の緑にしない）。既存のブロックと同じく `$failures.Count -eq 0` でガードし、前段が落ちていたら打鍵しない（窓が無い状態で打つと、前面の別アプリへ届く）
- 断言の失敗文言は、観測した並びと、疑う原因（文字の配送が `admit_key` より前へ動いた・#1274）を名指す
- **この層が見ないもの**（`AGENTS.md` の「検査・検証手段を新設」の行）: 物理キーボードのオートリピート（`keybd_event` はリピートを生まない・`ADR-synthetic-key-press-suppression`）。設定ウィンドウを閉じて戻る経路そのもの（#927 の再現経路）は通らない

## テスト方針と検証コマンド

- 実装前の確認（Red）: 新しいブロックが、いまの正しいコードで緑になる。この検査の Red は変異で作るので、変異注入は `/implement`「3b. 委譲へ渡すもの」の委譲先が行う
- 委譲先が注入する変異（`.claude/rules/safety-nets.md`: 設計を確定する前に注入する変異を書く）
  - 変異 2: `typed_text_event` の push を、`on_window_event` の `KeyboardInput` 腕で `admit_key` の呼び出しより前へ移す → 新しい断言の文言で赤になること
  - 手順 5（2 回目の down）を消す → それでも変異 2 が赤になるか（合成の文字だけで捕まえられるかの切り分け。赤の理由を出力で確かめる）
  - 対照: 変異なしで緑
- 検証（`docs/build-commands.md`「変更後の検証チェックリスト」）: カテゴリ C（`npm run smoke:egui`）、E（scripts の変更）、F（`npm run governance:check`）、A（`input.rs` のコメント変更に伴う fmt / clippy / test / doc）
- CI runner で `keybd_event` の押下が `GetAsyncKeyState` に映るかは、PR を立てて Smoke workflow で確かめる（PR 本文のチェックリストへ送る。計画の作業項目にしない）

## SPEC.md・関連文書の更新要否

- `SPEC.md`: 不要（挙動を変えない）
- ADR: 不要の見込み（却下した案 (A)(B) の理由は PR 本文と smoke のコメントに残す。否定の知識として残す価値が出たら `/implement` Step 4 で判断する）

## フェーズと作業項目

### フェーズ 1: smoke のブロック

- [x] `scripts/smoke-egui.ps1` の H1 判定の後に、手順 1〜9 のブロックを try/finally つきで足す
- [x] `:432-433` の「捕まえないもの」のコメントと `:783` の成功文言を直し、`:490-491` が真のままかを読み直す（真のまま。新しいブロックも Escape で隠して終える）
- [x] ローカルで `npm run smoke:egui` を通す（2026-10-24・緑。シナリオ 1 の trace で show/hide が各 3 件・区間 seq 121 より後の変化は `0->1` の 1 件だけで、新しいブロックが最後まで走ったことを確かめた）

### フェーズ 2: 呼び出し点のコメント

- [x] `snotra-egui-runtime/src/input.rs:410` のコメントに、smoke が捕まえる旨（#1274）を足す
- [ ] カテゴリ A・E・F の検証を通し、実装差分を確定させる

## 未確定（実装前に潰す）

- [x] Z を押したまま Alt+Q を打って hotkey が発火するか — **発火する**。2026-10-24 にローカルで probe（scratchpad の `probe-1274.ps1`・作り直した release バイナリ）を 1 回走らせ、Z を押したまま `egui_hide:done`（seq 35）と `egui_show:done`（seq 37）がどちらも増分で現れた
- [x] 押しっぱなしの Z への 2 回目の down が、非合成の press として届いて落ちるか — **届いて落ちる**。同じ probe（`SNOTRA_EGUI_INPUT_TRACE=1`）で、show の 4ms 後に `drop_key state=down physical=KeyZ synthetic=true reason=held_since_focus_gain`（合成 press）、2 回目の down で `drop_key … synthetic=false …` が出た。hide のときは tao が合成した Z の release が `push_key state=up` として通っていた
- [x] show 直後の reset が `egui_input:changed` を出さないか — **出さない**。同じ probe で、hide（seq 35）より後の `egui_input:changed` は `0->1`（X の打鍵・seq 55）の 1 件だけだった

## セルフレビュー

- リスク: 通常（検査の追加のみ。永続形式・並行境界・UI モードは触らない）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査）
- 要対処: 敵対的調査から 8 件を採用（`research.md` の採否表）。advisor から 2 件を採用（手順 2 で文字の入力を待つ・release バイナリの作り直し）
- 未検証: CI runner での `GetAsyncKeyState` の振る舞い（PR で確かめる）。未確定 3 件はローカルの probe で潰した
- 照合: issue の要件 2 つ（文字が入らないことを正の効果つきで断言する／変異 2 で赤になることを確かめる）はフェーズ 1 と委譲の変異に対応する。対称ペア（show ↔ Escape の hide・Z の down ↔ finally の up）は手順の中で閉じる

## 人間レビュー

- [x] 承認済み — 2026-10-24 / 問い: "計画の本文は `workspace/plan.md` です。注釈を書き足していただくか、承認していただけますでしょうか。" / 回答: "OK"

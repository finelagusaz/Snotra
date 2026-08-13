# 実装計画: #1038 — flush-on-Enter の判定を「armed」から「未反映」へ替える

調査は `workspace/research.md`、敵対的調査は `workspace/adversarial-1038.txt`。

## 目的

`should_flush_on_enter` の第 3 引数を `Debouncer::is_armed()`（「trailing を予約中か」）から
**「最終クエリの結果がまだ行へ反映されていないか」**へ替え、trailing 発火の直後
（`armed == false` かつ worker in-flight）の Enter が最終クエリでない行を起動する欠陥を塞ぐ。

## 受け入れ条件

| # | 条件 | 担保 |
|---|---|---|
| 1 | 「`armed == false` かつ in-flight あり」で flush 判定が真になる | `search_dispatch.rs` の `is_unsettled` ユニットテスト（リテラルと実 `SearchDispatch` の両方から接地） |
| 2 | その状態で `on_enter` が最終クエリの結果で起動する | 受け入れ 1 + **flush 枝の本体（`launcher_controller.rs:1322–1340`）を 1 行も変えない**ことの論証（`on_enter` にテスト席は無い・下「テスト方針」） |
| 3 | flush の頻度増が体感に乗らない | **費用の上界の論証**（下「不変条件と異常系」）。同期 flush は #631 以来の既存経路であり新設ではない |

## 設計判断

**合成（`armed || pending_seq != 0`）を名前のある純粋関数へ出す。**

```rust
// src-tauri/src/egui_shell/search_dispatch.rs
pub fn is_unsettled(armed: bool, pending_seq: u64) -> bool {
    armed || pending_seq != 0
}
```

呼び出し点:

```rust
if crate::egui_shell::should_flush_on_enter(
    self.state.view_kind(),
    is_plain,
    crate::egui_shell::is_unsettled(
        self.search_debounce.is_armed(),
        self.dispatch.pending_seq(),
    ),
) {
```

**理由**:

- `should_flush_on_enter` の第 3 引数は既に素の `bool` ゆえ、**その関数のテストは合成を一切測らない**。
  合成を呼び出し点の式のまま書くと、受け入れ 1 を測れる単位が消える
- `pending_seq: u64` を受けて `!= 0` を関数の内側に置く。**sentinel（0 = in-flight なし）が
  この修正で最も微妙な部分**であり、そこをテストの届く場所へ入れる
- 置き場所は `search_dispatch.rs`——sentinel の定義（`pending_seq` の `map_or(0, ..)`）と
  同じファイルに置き、**#1039 が `is_settled()` を型の内側へ作るときそのまま引っ越せる**形にする
- **`should_flush_on_enter` は 3 引数のまま**、第 3 引数名を `armed` → `unsettled` へ改める。
  この関数の責務は view/intent のゲートであって、sentinel の解釈ではない

## 変更ファイル一覧と対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/search_dispatch.rs` | `is_unsettled`（新規） | 追加 + doc + テスト 2 本 |
| `src-tauri/src/egui_shell/search_state.rs` | `should_flush_on_enter` | 第 3 引数名 `armed` → `unsettled`、doc を更新。テスト `flush_on_enter_only_for_armed_plain_results` の名前とメッセージを追随 |
| `src-tauri/src/egui_shell/mod.rs` | `pub(crate) use search_dispatch::{...}` | `is_unsettled` を re-export に追加（消費者コメントも更新） |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `on_enter`（1311–） | 第 3 引数を差し替え、#631 のコメントを「trailing 窓内」から「未反映」へ更新。**flush 枝の本体（1322–1340）は変更しない** |
| `docs/architecture.md` | 「検索フロー（入力 → 結果表示）」 | mermaid の `opt` ラベルと補足「例外は Enter である」の条件を更新 |

**触らない**: `poll_search_debounce` の repaint 再要求（`launcher_controller.rs:1299`）、
`is_search_armed` → `settled` → icon worker ゲート、`SPEC.md`、`docs/superpowers/specs/`。
理由は `research.md`「4. `is_armed()` の消費点」と「5. 文書側の記述」。

## 実装順序

### Phase 1 — 純粋核と検知器

- [x] `search_dispatch.rs` に `is_unsettled(armed: bool, pending_seq: u64) -> bool` を追加する。doc に
      次の 2 点を書く
  - 「`armed` を残すのは leading 発火の前（要求がまだ出ていない瞬間）を覆うため」
  - **#1039 への申し送り**: この述語は #1039 で `SearchState::is_settled()` として型の内側へ移る。
    **否定形で置いたのは呼び出し点に `!` を出さないため**であり、#1039 の issue 本文が想定する
    肯定形 `is_settled()` とは**極性が逆である**（引っ越し時は `!is_unsettled(..)` として吸収する）
- [x] `is_unsettled` のユニットテストを 2 本書く
  - `unsettled_covers_in_flight_after_trailing_fired`: リテラル 4 通り
    （`(false, 0) == false` / `(true, 0) == true` / **`(false, 1) == true`（受け入れ 1）** / `(true, 3) == true`）
  - `unsettled_is_grounded_on_real_dispatch`: 実 `SearchDispatch` を `issue` → `pending_seq()` を渡して真、
    `accept` 後に偽、`invalidate` 後に偽。**リテラルではなく型自身から sentinel を取る**
      （入力が出力側へすり替わる不動点化を避けるため、`armed` 側は `false` 固定で渡す）
  - 上のいずれかに **受け入れ 1 を逐語で写した合成アサーション**を 1 行加える:
    `assert!(should_flush_on_enter(ViewKind::Results, true, is_unsettled(false, 1)))`
    （issue の受け入れ 1「`armed == false` かつ in-flight あり」をコードのまま固定する）
- [x] `search_state.rs` の `should_flush_on_enter` の第 3 引数名と doc を改める。既存テストの
      4 ケースはそのまま真理値が保たれる（引数の意味替えのみ）ので、名前とメッセージだけ追随させる

### Phase 2 — 呼び出し点

- [x] `mod.rs` の `pub(crate) use search_dispatch::SearchDispatch;` を `{SearchDispatch, is_unsettled}` へ広げ、
      消費者コメントに「`on_enter` の flush 判定（#1038）」を足す
- [x] `launcher_controller.rs:1317–1321` の第 3 引数を `is_unsettled(is_armed(), pending_seq())` へ差し替える
- [x] 同 1312–1314 のコメントを更新する（「trailing 窓内（打鍵後 50ms 以内）」→
      「最終クエリの結果が未反映（trailing 予約中 **または** worker in-flight）」）

### Phase 3 — 変異注入（検知器が実際に落ちることを測る）

- [x] `is_unsettled` から `|| pending_seq != 0` を一時的に外し、`cargo test -p snotra` が
      **Phase 1 の 2 本とも落ちる**ことを確認する。落ちない項があればテストを直す
      （**TDD の Red と同一の観測になったので順序を入れ替えた**——先に `armed` のみの実装を置き、
      呼び出し点まで配線した状態でテストを当てて落とした。変異を後から入れるのと測るものは同じで、
      加えて「呼び出し点の配線が済んだ状態で落ちる」ことまで確かめられる）
- [x] 変異を戻し、緑に戻ることを確認する
- [x] 落ちたテスト名と exit code を本ファイルの「変異注入の記録」へ書く

### Phase 4 — 文書と検証

- [ ] `docs/architecture.md` の mermaid `opt Enter が trailing 窓（50ms）内に来た Plain（should_flush_on_enter）`
      を新しい条件へ書き換える
- [ ] 同「**例外は Enter である**——trailing 窓内の Enter は…」の補足を更新し、
      **窓が worker の走査時間（実運用点で 40〜95 ms・#1036）ぶん広がったこと**と、その受容理由を書く
- [ ] カテゴリ A を実行する: `cargo fmt --all -- --check` / `cargo check --workspace` /
      `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` /
      **`cargo doc --workspace --no-deps --document-private-items`（hook 非発火・手動必須）**
- [ ] カテゴリ F を実行する: `npm run governance:check`（`docs/architecture.md` を触るため）
- [ ] `docs/architecture.md` の他の言及を確認する（`rg -n "flush|armed" docs/architecture.md`）——
      とくに「その 2 つには in-flight が残りうる…**窓が開くのは flush が発火しない条件に限る**」の
      補足が、書き換えた Enter の例外と矛盾しないこと（#1038 はこの残余を**狭める**方向なので
      文言はそのまま真のはずだが、3 箇所目の腐りを見落とさないため実測する）
- [ ] 実装差分を確定させる（`git diff main --name-only` で変更ファイルが上表の 5 件に閉じていることを確認する）

## 不変条件と異常系

| # | 不変条件 | 検知手段 |
|---|---|---|
| I1 | `pending_seq() == 0` ⇔ in-flight なし（seq は 1 始まり） | `unsettled_is_grounded_on_real_dispatch`（`issue` / `accept` / `invalidate` を実型で通す） |
| I2 | flush 枝の本体は未変更 | `git diff main -- src-tauri/src/egui_shell/launcher_controller.rs` で 1322–1340 に変更が無いこと。**引数 1 個の `git diff` も `main...HEAD` も使わない**——前者は作業ツリーだけ、後者はコミット同士の比較ゆえ、フェーズごとにコミットすると片方が自明に緑になる（#922） |
| I3 | `Results` / `Plain` 以外では flush しない | `should_flush_on_enter` の既存 4 ケース（Folder / Tool / instant）が緑のまま |
| I4 | repaint 再要求へ `pending_seq` を混ぜない | `launcher_controller.rs:1299` を触らない（I2 と同じ形の `git diff main -- <path>` で確認）。混ぜると `request_repaint_after(ZERO)` の空回り（worker 死亡時は無限） |

**異常系**:

- **空クエリ / indexing 中に Enter**: flush 枝は `None` → `dispatch.invalidate()` + `set_results(Vec::new())`
  へ落ちる（既存コード・1326–1336）。判定が広がっても**この枝の入力集合は変わらない**
  （`is_plain` と `view_kind` のゲートは不変）
- **worker 死亡**: `pending` が固着し `is_unsettled` が真のまま留まる。帰結は
  「Enter が毎回同期 flush を払う」＝**worker 無しで最終クエリの結果を出す**。
  現行（`armed` だけ）はこの状況で flush せず古い行を起動し続けるため、**新判定の方が安全側**である
  （`research.md`「worker 死亡時の劣化」）
- **単打鍵バーストの偽陽性**: trailing が同じクエリを再発行した in-flight 中の Enter は、
  結果が同じまま同期 flush を払う。**費用増だけで挙動は変わらない**。根治は #1039 の領分
  （`research.md`「既知の偽陽性」）

**この値で初めて走る行**（`AGENTS.md`「どの分岐が選ばれるかを決める値の出所を変更」）:

判定が新しく真になるのは `Results ∧ Plain ∧ armed == false ∧ pending != 0` のみ。この状態を
既存の全モードと突き合わせた結果（`/state-check`）、**挙動が変わるのは `indexing` との組み合わせ 1 つだけ**である。

- **Enter × 構築中 × in-flight**: 現行は flush せず `state.results()` の古い行を起動する。ところが
  §4.7 の表示ゲート（`plain_results_hidden`）が構築中の plain 結果を隠すため、**画面に見えていない行が
  起動する**。新判定では flush 枝が `self.indexing()` → `None` → `set_results(Vec::new())` へ落ちて
  行が空になり、起動しない。**改善方向であり SPEC §4.7 と整合する**（`SPEC.md` の更新は不要——
  §4.7 は表示の規定であって Enter の規定ではなく、記述を変えない）
- `Folder` / `Tool` は `view_kind` の連言が、`Instant` / `Command` は `is_plain` の連言と
  各分岐の `invalidate()` が塞ぐ。`launching` 中は `start_launch` の `invalidate()`（`:274`）と
  入力欄の無効化（SPEC §18「egui 経路の起動保護」）により**到達しない**

**費用の上界（受け入れ 3 の論証）**: flush が発火しうる窓は
「打鍵 → 50 ms（trailing 予約中）」から「打鍵 → 50 ms + worker の走査時間（40〜95 ms・#1036）」へ広がる。
1 回あたりの費用は変わらない（同じ同期 `engine.search`）。Enter は結果を確定させる 1 回だけであり、
打鍵ごとの費用ではない（`docs/architecture.md` が受容として記録済み）。**新設の費用は無い。**

## テスト方針と検証コマンド

- **受け入れ 1** = `search_dispatch.rs` の新規テスト 2 本（Phase 1）+ **変異注入で落ちること**（Phase 3）
- **受け入れ 2** に**テスト席は無い**——`launcher_controller.rs` に `mod tests` は存在せず
  （`rg -n "mod tests" src-tauri/src/egui_shell/launcher_controller.rs` が 0 件・実測）、
  `on_enter` は `AppHandle` と `AppState`（engine lock）を要求する。**ハーネスを新設しない**。
  担保は「判定の入力だけが変わり、flush 枝の本体（#631/#1004 の既存コード）は不変」（I2）である
- **実機再現は範囲外**。issue 自身が「これはコードからの導出であって、実機で再現していない」
  「trace の書き込みが 1 本約 10 ms かかるため計器が窓を歪める」と記しており、
  受け入れ 1〜3 のいずれも実機再現を要求していない
- 検証コマンドは `docs/build-commands.md` カテゴリ A（`*.rs`）+ F（ガバナンス文書）。
  `cargo doc` と `npm run governance:check` は **PostToolUse hook が発火しないので手で走らせる**

## SPEC.md・関連文書の更新要否

| 文書 | 要否 | 根拠 |
|---|---|---|
| `SPEC.md` | **不要** | flush-on-Enter / trailing 窓 / Enter の debounce 例外の記述が無い（grep 実測）。記載のフロー・状態遷移を変えないので**バグ修正**である |
| `docs/architecture.md` | **必要** | 「検索フロー」の mermaid と補足が条件を逐語で持つ |
| `docs/superpowers/specs/2026-08-10-search-worker-design.md` §4.7 | **不要** | `docs/superpowers/README.md:5` が「各時点の設計書のスナップショット。…更新されない」と明記 |
| `src-tauri/CLAUDE.md` | **不要** | flush-on-Enter の条件を持たない（新しい横断不変条件も生じない） |

## 未確定（実装前に潰す）

- [x] **合成をどこへ置くか** — `should_flush_on_enter` は第 3 引数が素の `bool` ゆえ合成を測れないことを
      コードで確認（`search_state.rs:415`）。`search_dispatch.rs` に純粋関数 `is_unsettled` を新設し、
      sentinel の解釈（`!= 0`）をその内側へ入れると決めた。**却下案**: (a) 呼び出し点の式のまま
      → 受け入れ 1 を測る単位が消える。(b) `SearchDispatch` のメソッド `has_in_flight()` だけ足す
      → 合成が呼び出し点に残り (a) と同じ。(c) `should_flush_on_enter` を 4 引数にする
      → 責務が混ざり、#1039 で第 3 引数の**生産者**を差し替えるときに触る面が増える
- [x] **述語の名前** — #1039 が `is_settled()` を型の内側へ作ると宣言しているため、否定形
      `is_unsettled` を採る（呼び出し点に `!` が現れない側）。#1039 では `!is_unsettled(..)` を
      `is_settled()` として吸収できる
- [x] **debounce の 50 ms は環境依存か** — **依存しない**。`Duration::from_millis(50)` の
      ハードコード 2 箇所（`launcher_controller.rs:149` / `984`・`rg -n "from_millis\(50\)"` で自ら実測）。
      実機 `config.toml` に debounce のキーは無い（3b が実機ファイルを読んで確認）
- [x] **`pending_seq == 0` sentinel は健全か** — `issue` が `next_seq += 1` を先に行うため最初の seq は 1。
      `pending` を消す経路は `accept`（seq 一致時のみ）/ `invalidate` の 2 つで、worker 送信失敗枝
      （`launcher_controller.rs:790–804`）と hide/show 跨ぎ（同 980 `consume_reset_pending`）は
      どちらも `invalidate` を撃つ。残る唯一の固着経路（worker 死亡）は上「異常系」で安全側と判定した
- [x] **`is_armed()` の他の消費点へ波及するか** — 3 系統を列挙し、repaint 再要求（触ると永久スピン）と
      icon worker ゲート（無駄仕事のみ・#1039 の領分）を**触らない**と決めた（`research.md` の表）

## セルフレビュー

- リスク: **通常**
  - 永続形式・識別子/キー形式の変更なし（`/persistence-check` 非該当）
  - 並行境界の新設なし（`/race-check` 非該当）——`pending_seq()` は既存の `is_armed()` と
    **同じスレッドの同じフレームで読む**同期的な読み取りであり、worker・channel・listener・
    live-read のいずれも追加しない。**判定は入力が 1 つ増えるだけで、`on_enter` の副作用の
    順序は変わらない**
  - 網羅性が要件の変更ではない（`/plan-review --deep` 非該当）
- plan-review: **未実施（通常リスク）** / 自己レビューのみ
- 該当した check スキル: `/state-check`（ガード条件の変更）・`/dry-check`（関数の新規定義）——**両方とも実行済み**
  - `/dry-check`: 置換対象 **0 件**。`pending_seq() != 0` の手書き比較は 0 件（既存 2 件はいずれも
    trace の出力フィールド）。`view.rs:1120` の `let settled = !is_search_armed()` は同型に見えるが
    **[維持]**——perf ヒューリスティックであって正しさの述語ではなく、当てると worker 走査中の
    アイコン取得が 40〜95 ms 遅れて悪化する。「片方だけが変わる将来」を挙げられる（#1039 で
    `is_settled()` が来歴まで見ても icon ゲートは「連打中か」のままでよい）＝別概念
  - `/state-check`: **不整合なし**。直交性マトリクス 5 組（Folder / Tool / Instant・Command /
    indexing / launching）、リセット経路 4 本（show / Escape 連鎖 / 起動完了 / モード離脱）、
    入力分岐（Enter・Shift+Enter は同じ flush を通る／クリックは射程外）、SPEC §8.6 との整合
    （flush はモード遷移ではないので図の更新不要）を確認。**要対処 1 件を本文へ反映**（上記
    「この値で初めて走る行」の indexing の項）
- エージェント数: **1**（Step 3b の敵対的調査のみ）
- 要対処: 3b の所見 5 件のうち 5 件採用（A: sentinel の根拠列挙を hide/show・panic まで拡張 /
  B: 「永久スピン」を worker 死亡時のみ無限と精密化し、劣化モードが安全側であることを追記 /
  C: `mod tests` 不在の実測 / D: #1039 の 3 経路が `ViewKind::Results` ガードと直交 /
  E: 50 ms がハードコードであることの実測）。**壊せた項目 0 件。**
  B の但し書き「worker 死亡は現状到達不能」だけは**採らなかった**——`search_worker.rs` の doc が
  engine lock の毒（debug/test）を到達しうる死因として名指しているため
- 未検証: 実機再現（issue が範囲外と規定・上「テスト方針」）

## 5a 自己照合（主エージェント）

1. **issue の全要件に作業項目が対応する** — 受け入れ 1 → Phase 1/3、2 → Phase 2 + I2、3 → 費用の上界の論証 + Phase 4 の文書更新
2. **境界条件を列挙し、各条件に検証がある** — `(armed, pending)` の 4 象限をリテラルテストが覆う。
   view/intent 側の境界（Folder / Tool / instant）は既存 4 ケースが維持する
3. **新しい状態・リソース・プロセスに正常/失敗/破棄経路がある** — **新しい状態を作らない**
   （既存の 2 つの状態を読む純粋関数のみ）。ゆえに生成/破棄のペアは生じない
4. **より単純な既存パターンで置き換えられないか** — 呼び出し点に式を書く案が最も単純だが、
   受け入れ 1 を測れなくなる（未確定 1 の却下案 (a)）
5. **壊してはならない不変条件に検知手段がある** — I1〜I4（うち I2/I4 は `git diff`、I1/I3 はテスト）。
   **I1 は変異注入で発火することまで確かめる**（Phase 3）

## 変異注入の記録

**変異**: `is_unsettled` の本体を `armed || pending_seq != 0` ではなく `armed`（＝現行 main の判定と
等価な、性質をまだ持たない実装）にする。`_pending_seq` は未使用。

**測った状態**: 呼び出し点（`launcher_controller.rs` / `mod.rs` の re-export）まで**配線済み**。
clippy は緑（`dead_code` 解消済み）で、落ちるのはテストだけという状態で観測した。

**結果**: `cargo test -p snotra` → **exit 101**、`254 passed; 2 failed`

```
failures:
    egui_shell::search_dispatch::tests::unsettled_covers_in_flight_after_trailing_fired
    egui_shell::search_dispatch::tests::unsettled_is_grounded_on_real_dispatch
```

- `unsettled_covers_in_flight_after_trailing_fired` — assert メッセージ
  「trailing 発火の直後（armed == false かつ in-flight あり）が #1038 の欠陥そのものである」で停止
- `unsettled_is_grounded_on_real_dispatch` — assert メッセージ「worker へ出した直後は未反映」で停止

**変異を戻した後**: `cargo test -p snotra` → **256 passed; 0 failed**（新規 2 本が加わった数と一致）。

**判定**: 検知器は**新設した 2 本とも**この変異で落ちる。`|| pending_seq != 0` を消す退行は
沈黙せず捕まる。

## 引き継ぎ（この計画が所有しない作業・チェックボックスを置かない）

**作業項目ではない。** コミットより後に起きる行為ゆえ、`- [ ]` を置くとどちらへ転んでも
契約（未確定欄が空・`gh pr create` の未チェックガード #749）に触れる。ここは意図の記録である。

- **#1039 へ、述語の名前と極性が issue 本文の想定と違うことを伝える。** #1039 の本文は
  `is_settled()`（肯定形・型の内側）を前提に書かれているが、本 issue が置くのは
  `search_dispatch::is_unsettled(armed, pending_seq)`（否定形・自由関数）である
- **一次の機構はコードの doc コメントである**（Phase 1）——#1039 の実装者が最初に読む場所に置く。
  issue コメントは補助であり、**マージ後に `gh issue comment 1039` で添える**
- ユーザー承認済み（下記「人間レビュー」の回答が根拠）

## 人間レビュー

- [x] 承認済み — 2026-08-13 / 問い: "この計画で実装へ進めてよろしいですか。注釈があれば `workspace/plan.md` へ直接お書き添えください。"（および「述語を否定形 `is_unsettled` で置くか、#1039 の宣言に合わせて肯定形 `is_settled` にして呼び出し点で `!` を付けるか」） / 回答: "is_unsettled で OK、作業完了時に#1039にコメントで名前が変わっている旨をコメントするか何かして、#1039でスムーズに作業できればいい"

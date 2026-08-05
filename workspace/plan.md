# plan — #927 focus 獲得時の合成 press で本体が hide される

## 目的

**focus を獲得した瞬間に既に押されていたキーは、release されるまで press を egui へ渡さない。** これにより「設定窓を Escape で閉じたら本体も一緒に hide される」（#927）が消える。射程は Escape に限らず全キー（↑↓・文字キーの混入も同じ穴・実測）。

機序の確定と一次証拠は `workspace/research.md`（要旨は issue #927 のコメント）。

## 受け入れ条件

1. 設定窓で Escape を押して閉じたとき、**本体は hide されない**（`egui_hide:done` が 0 件）——`scratchpad/repro-927.ps1` を `-EscapeHoldMs 60` と `-EscapeHoldMs 900` で実行して確認する
2. 本体が可視・focus 中の Escape は**従来どおり hide する**（`npm run smoke:egui` が緑）
3. 人の実打鍵で、設定窓の Escape を **1 秒押しっぱなし**にしても本体が残る（物理オートリピート (A) の実在有無に関わらず受け入れ条件が満たされる）
4. 抑止は focus セッションを越えない（release が届かない経路でも次の focus で Escape が効かなくなることがない）

## 変更ファイルと対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-egui-runtime/src/runtime.rs` | `rx_key` trace | **実施済み**（下の「計器の先行投入」）。`synthetic={is_synthetic}` を追加 |
| `snotra-egui-runtime/src/input.rs` | `InputState`（構造体） | フィールド `held_since_focus_gain: HashSet<KeyCode>` を追加（`new()` も） |
| 同上 | `admit_key`（**新規・純粋関数**） | 「この キーイベントを egui へ渡すか」の判定。`held` を更新して bool を返す |
| 同上 | `InputState::on_window_event` の `WindowEvent::KeyboardInput` arm | `is_synthetic` を受け、`admit_key` が false なら `on_keyboard_event` を呼ばず `drop_key` trace を出す |
| 同上 | 同 `WindowEvent::Focused(focused)` arm | `focused == false` のとき `held_since_focus_gain.clear()` |
| 同上 | `mod tests` | 下の「テスト方針」の 7 件 |
| `snotra-egui-runtime/CLAUDE.md` | 「不変条件 / 一般」 | 合成 press の抑止と、tao のイベント順序（下記）を 1 項目で記す |

**触らない**: `src-tauri/src/egui_shell/view.rs`（Escape ラダーは正しい）・`snotra-settings/`（Escape 閉じは維持・是非は別 issue）・`SPEC.md`（文書化された挙動を変えないため——「Escape で非表示」§8.1 は不変）。

## 計器の先行投入（実施済み・2026-08-05）

**`admit_key` の第 1 分岐は「その Escape press が `is_synthetic == true` で届く」ことに全面的に依存する。** これは 3 つの状況証拠（tao ソース・CapsLock/Backquote の合成 release・1ms 対照）からの**推論**であって、`rx_key` / `push_key` のどちらも `is_synthetic` を出していなかった。偽なら gate は完全な no-op になり、フィールド・純粋核・テスト 7 件・CLAUDE.md をすべて書いた後に空振りする。

ゆえに実装より前に `runtime.rs` の `rx_key` へ `synthetic={is_synthetic}` を足し、`repro-927.ps1 -InputTrace -EscapeHoldMs 60` で 1 回測った:

```
rx_key … state=Pressed  physical=Escape    synthetic=true   ← 本体が受けた Escape の press
[trace] egui_hide:done
rx_key … state=Released physical=Escape    synthetic=true
rx_key … state=Released physical=CapsLock  synthetic=true
rx_key … state=Released physical=Backquote synthetic=true
```

**確定**: 本体が受ける Escape press は `synthetic=true` である。`/o` の打鍵（Slash / KeyO）は `synthetic=false` で、計器が両者を弁別できることも同じログが示している。この項目は恒久的に残す（`repeat` / `mapped` と同じ「落とした側も残す」規律）。

## 実装の骨格

```rust
/// キーイベントを egui へ渡すか（#927）。true = 渡す。`held` は副作用として更新する。
///
/// tao は `WM_SETFOCUS` で「その瞬間に押されている全キー」の合成 press を作る
/// （tao-0.35.3/src/platform_impl/windows/keyboard.rs:87-93）。設定窓を Escape で
/// 閉じた直後に本体が focus を取り戻すと、この合成 press が Escape ラダーを走らせる。
fn admit_key(is_synthetic: bool, pressed: bool, physical: KeyCode, held: &mut HashSet<KeyCode>) -> bool {
    if !pressed {
        held.remove(&physical);
        return true; // release は常に渡す（egui の keys_down を汚さない）
    }
    if is_synthetic {
        held.insert(physical);
        return false;
    }
    !held.contains(&physical) // 押しっぱなしの間の物理リピートも落とす
}
```

## 不変条件と異常系

- **`Focused(true)` では clear しない。** tao は synthetic key events を `public_window_callback_inner` の `keyboard_callback`（`event_loop.rs:967-993`）で送り、`Focused(true)` はその**後**の `match msg` → `gain_active_focus`（同 `:870-878`・`:1792`）で送る。**合成 press は `Focused(true)` より先に届く**ため、`Focused(true)` で clear すると直前に立てた抑止を消してしまう
- **`Focused(false)` で clear する。** KILLFOCUS 側も合成 release が先・`Focused(false)` が後（`lose_active_focus`・`:880-891`）なので、順序に関わらず集合は空で終わる。これが受け入れ条件 4 の実体（release が届かない経路でも抑止が持ち越されない）
- **release は落とさない。** 落とすと egui の `keys_down` に押しっぱなしが残る。**egui が見たことのない press に対する release を渡すことも安全である**——`key_released` / `keys_down` を読む消費者は `src-tauri/src/egui_shell/` にも `snotra-egui-runtime/src/` にも **0 件**（grep 実測・2026-08-05）で、読み手は egui 内部（TextEdit 等）だけである
- **modifiers は無影響。** egui の modifiers は `ModifiersChanged` から作る（`modifiers_from_tao`）
- **文字入力は無影響。** `ReceivedImeText` は別経路（`committed_text_event`）
- **落としたことを残す**（#872/#936 の規律）。`drop_key` trace に `physical` / `state` / `synthetic` を出す——出さないと「届いたが渡さなかった」が沈黙になる
- **fail-closed の危険を検査で固定する**: 抑止が解けず Escape が永久に効かなくなる形を、`admit_key` の release テストと `Focused(false)` の clear テストで押さえる

## 合格条件を連言にする理由

`MainHidden: False` **単独では合格条件にならない**——同じ値を**修正前のビルドで既に観測している**（1ms の対照試行）。単独で読むと「gate が効いた」と「注入のタイミングがずれて合成 press がそもそも来なかった」が区別できず、`RETROSPECTIVE.md`「故障注入が本来の回帰より強く、縛れていない検査を『縛れている』と記録した」と同じ形になる。**`drop_key` の出現が「再現条件は成立していた」の肯定的証拠**であり、これと hide 0 件の連言だけが gate の作動を示す。`repro-927.ps1` はこの 2 つを両方判定する形へ更新する。

## テスト方針

`snotra-egui-runtime/src/input.rs` の `mod tests`（**`WindowEvent::KeyboardInput` は `#[non_exhaustive]` で crate 外から構築できない**ため、判定は純粋関数で駆動する。`WindowEvent::Focused(bool)` は構築できるので clear は `on_window_event` 経由で測る）:

1. 合成 press は渡らず、集合へ入る
2. 抑止中の**非合成** press（物理リピート相当）も渡らない
3. release は渡り、集合から抜ける
4. release 後の press は渡る（＝Escape が永久に効かなくならない）
5. 別のキーの press は抑止を受けない
6. 合成 release は渡る（集合を触っても害が無い）
7. `on_window_event(&WindowEvent::Focused(false))` で集合が空になる

## 検証コマンド

- `cargo test -p snotra-egui-runtime`（カテゴリ A・post-edit hook も自動実行）
- `cargo build --release -p snotra -p snotra-settings`
- `pwsh -File <scratchpad>/repro-927.ps1 -InputTrace -EscapeHoldMs 60` → `MainHidden: False`
- 同 `-EscapeHoldMs 900` → `MainHidden: False`
- `npm run smoke:egui`（通常の Escape hide が生きていること・受け入れ条件 2）
- `npm run governance:check`（`CLAUDE.md` を触るため）
- 人の実打鍵（受け入れ条件 3・`SNOTRA_TRACE=1` + `SNOTRA_EGUI_INPUT_TRACE=1` で `egui_hide:done` が 0 件）

## 文書の更新要否

- `SPEC.md`: **不要**。文書化された挙動（`Escape` で非表示・§8.1）は変えない。入力層の欠陥修正である
- `snotra-egui-runtime/CLAUDE.md`: **要**（不変条件 1 項目・上の「`Focused(true)` では clear しない」を含む——コードだけでは tao のイベント順序が見えないため）
- `docs/`: 不要
- issue #927: 受け入れ条件が (A)/(B) 二択のままなので、(C) 前提へ差し替える旨は既にコメント済み

## 作業項目

### Phase 1 — 実装とユニットテスト

- [ ] `/symmetric-check` をインライン実行し、insert / remove / clear の 3 経路を検算する
- [ ] `InputState` に `held_since_focus_gain` を追加（`new()` の初期化を含む）
- [ ] `admit_key` を実装（doc に tao の一次資料 file:line を書く）
- [ ] `on_window_event` の `KeyboardInput` arm を `is_synthetic` 受けに変え、落とすときは `drop_key` trace
- [ ] `Focused(false)` で `clear()`
- [ ] テスト 7 件を追加
- [ ] `cargo test -p snotra-egui-runtime` が緑

### Phase 2 — 実機検証

- [ ] `repro-927.ps1` を連言判定へ更新（`drop_key physical=Escape synthetic=true` の件数も数え、0 件なら「再現条件が成立しなかった」として赤にする）
- [ ] `cargo build --release -p snotra -p snotra-settings`
- [ ] `repro-927.ps1 -EscapeHoldMs 60` / `-EscapeHoldMs 900` が、**同一の走行で次の 2 つを同時に**満たす（連言・下の「合格条件を連言にする理由」）
  - `egui_hide:done` が 0 件（`MainHidden: False`）
  - `drop_key` に `physical=Escape synthetic=true` が **1 件以上**現れる
- [ ] `npm run smoke:egui` が緑（従来の Escape hide が生きている）
- [ ] 人の実打鍵で Escape 1 秒保持 → 設定は閉じ、本体は残る

### Phase 3 — 文書

- [ ] `snotra-egui-runtime/CLAUDE.md` に不変条件を 1 項目追加
- [ ] `npm run governance:check` が緑

## 未確定（実装前に潰す）

- [x] hotkey（Alt+Q）で show した直後に届く Alt / Q の**合成 press が落ちること**が既存挙動を壊さないか — **壊さない**。文字入力は `ReceivedImeText` 経由（`input.rs` の `committed_text_event`）、modifiers は `ModifiersChanged` 経由で、どちらもキーイベントに依存しない。現行 trace でも Alt+Q 後にクエリは空のまま（実測ログ `push_key state=up physical=AltLeft repeat=true mapped=false` の直後に `egui_input:changed` は無い）。#938 が直したのは TextEdit の focus 順であってキー配送ではない
- [x] 抑止したキーの release が届かない経路で抑止が持ち越されないか — **持ち越さない**。`Focused(false)` で clear する。tao は KILLFOCUS で合成 release を先に、`Focused(false)` を後に送る（`event_loop.rs:880-891` / `:1799-1805`）ので、順序に関わらず空で終わる
- [x] `Focused(true)` でも clear すべきか — **すべきでない**（合成 press が `Focused(true)` より先に届くため・`event_loop.rs:967-993` が `match msg` より前にある。一次資料で確認）
- [x] **本体が受ける Escape press は本当に `is_synthetic == true` か** — **真**（実測・上の「計器の先行投入」）。これが偽なら gate は no-op だった
- [x] ユニットテストで `WindowEvent::KeyboardInput` を構築できるか — **できない**（`#[non_exhaustive]`・`tao-0.35.3/src/event.rs:349`）。判定を純粋関数へ切り出す形にした

## セルフレビュー

- トリガー処理（`AGENTS.md`「条件別チェック」）: **`/symmetric-check`**（生成/破棄ペア）が該当する。**インライン実行のスキルはサブエージェント方針に拘束されないため、実装フェーズの冒頭で 1 回インライン実行する**（作業項目 Phase 1 の 1 行目）。計画時点の代替として自己レビュー 3（insert / remove / clear の 3 経路）と不変条件節が同じ問いに答えている。**`/dry-check`** は grep で処理済み（自己レビュー 5）。**`/race-check`** は非該当（worker・channel・listener・共有状態のいずれも増えず、`InputState` はイベントループスレッドに閉じている）
- リスク: 通常（永続形式・公開 API・並行境界・網羅性のいずれにも当たらない。`CLAUDE.md` へ 1 項目足すが、規範の新設ではなくモジュール不変条件の記録）
- plan-review: 自己レビューのみ（**セッション方針でサブエージェント委譲を行わないため、独立レビューは起動しない**）
- エージェント数: 0
- 要対処: 4 件（上の未確定欄）——すべて一次資料で解消し、計画本文（不変条件・テスト方針）へ反映済み
- 未検証: 物理キーボードのオートリピート (A) の実在。注入では原理的に測れないため、受け入れ条件 3（人の実打鍵）で一括して満たす設計にした

### 自己レビュー 5 点

1. **issue の全要件に作業項目が対応する** — 機序の確定（済・コメント）／(A)(B) 以外だった場合の「より深い調査」（済）／対処の判断記録（本計画 + issue コメント）
2. **境界条件と検証** — 短い押下（1ms・現行でも hide しない）／長い押下（900ms）／別キー保持（Z）／通常の Escape hide（smoke:egui）／release が届かない経路（`Focused(false)` テスト）
3. **新しい状態の正常/失敗/破棄経路** — `held_since_focus_gain` は insert（合成 press）・remove（release）・clear（focus 喪失）の 3 経路をすべて持つ
4. **より単純な既存パターンで置き換えられないか** — 「合成 press だけ落とす」（状態ゼロ）と比較し、物理オートリピート (A) を塞げない点でユーザーが後者を却下（2026-08-05 の選択）。view 層で Escape だけ落とす案も同時に却下（↑↓・文字キーの混入が残る）
5. **既存の打鍵抑止と重複しないか（`/dry-check` 相当・grep 実測）** — この crate/shell でキーイベントを取り除いている箇所は `view.rs:319` の ↑↓ `events.retain`（#700）だけで、**そちらは「TextEdit にも効いてしまう」を防ぐウィジェット層の消費**であり、今回の「focus 獲得時に押されていたキー」とは概念が違う。統合しない
6. **壊してはならない不変条件に検知手段がある** — 「Escape で hide できる」は `smoke:egui`、「抑止が持ち越されない」はユニットテスト 4・7、「合成 press が来ない」は `repro-927.ps1`

## 人間レビュー

- [x] 承認済み — 2026-08-05 / 問い: "`workspace/plan.md` へ注釈を入れていただくか、このまま承認いただければ Phase 1 の実装へ進みます。" / 回答: "OK"

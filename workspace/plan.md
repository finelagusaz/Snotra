# plan: issue #711 — blur 猶予を「armed の間は毎フレーム再要求」へ揃える（契約③）

前提は `workspace/research.md` と設計 spec `docs/superpowers/specs/2026-07-26-frame-scheduling-contract-design.md` §5（案 A 確定・案 B/C の却下理由も記録済み）。type:fix / size:S。**通常経路の挙動は不変**（下記の失敗経路でのみ差が出る）。

## 失われ方の同定（issue の「潜在」を具体化・2026-07-26 実コード確認）

issue は「予約が早まる／フレームが落ちる要因が入れば再び顕在化する」と将来形で書いているが、**今日すでに具体的な喪失経路がある**:

`repaint.rs` の worker は `pending: Option<Instant>` に「最も早い deadline」を**単一スロット**で保持し、dispatch 時に `pending.take()` で**空にする**（`repaint.rs:175-185`）。ゆえに blur 後 100ms の予約中に**より早い要求が 1 つでも入ると、両者が 1 回の dispatch へ畳まれ、100ms の deadline は黙って消える**。

より早い要求は実在する——`register_config_wake_listeners` の `wake_main`（`CONFIG_APPLIED` / `INDEXING_STARTED` / `INDEXING_COMPLETE`）は**フォーカスと無関係に**発火し、`WindowWaker::wake()` = `request(ZERO)` である。その wake で走るフレームでは経過が 100ms 未満ゆえ `blur_should_hide` は false、そして現行コードには再要求が無い → **hide は次の無関係な入力まで宙吊り**。

頻度は低い（blur 直後 100ms 以内に index build 完了や config 保存が重なったとき）が、**再現手順を持たない「たまに勝手に隠れない」**として現れる型である。本修正はこの経路を閉じる。

## 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `src-tauri/src/egui_shell/lifecycle.rs` | ① `pub(crate) const BLUR_GRACE: Duration = Duration::from_millis(100)`（view.rs に 2 箇所手書きされていた値の一本化） ② 3 値 enum `BlurAction { Hide, Rearm(Duration), Idle }` と純粋核 `blur_grace_action(elapsed, focused, auto_hide, settings_running)`（TDD Red→Green・`blur_should_hide` を内部で再利用し二重定義しない） |
| `src-tauri/src/egui_shell/mod.rs` | `lifecycle` の re-export に `BlurAction` / `blur_grace_action` / `BLUR_GRACE` を追加（`blur_should_hide` と同じ行の並び。**`blur_should_hide` の re-export は残す**——純粋核の意味は消えず、`blur_grace_action` の内部で生きる） |
| `src-tauri/src/egui_shell/view.rs` | blur 猶予節（:1321-1338）を `blur_grace_action` の 3 値 match へ。`Rearm(remaining)` で `ctx.request_repaint_after(remaining)`（契約③）。エッジの初回予約は `BLUR_GRACE` を使う形で残す |
**当初含める予定だった `commands/window.rs` の `wake_main` 追加は取り下げた**（下記「settings 例外の扱い」）。

## 実装

```rust
// lifecycle.rs
/// blur（focus 喪失）から hide 判定までの猶予（#532 SU2）。**予約と判定の両方がこの値を使う**
/// ——片方だけ変えると「予約は 100ms 後・判定は別の閾値」の静かな不整合になる。
pub(crate) const BLUR_GRACE: Duration = Duration::from_millis(100);

/// blur 猶予のこのフレームでの処置（#711・契約③）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BlurAction {
    /// 猶予明け + 条件成立 → hide を要求する。
    Hide,
    /// 猶予中 → 残余で再要求する（armed の間は毎フレーム・早着/消失への耐性）。
    Rearm(Duration),
    /// 猶予明けだが条件不成立（auto_hide off / 設定サイドカー起動中）→ 何もしない。
    /// **再要求しない**——時間の経過では状態が変わらず、変化は別経路（focus 復帰・
    /// config 適用）が自らフレームを起こすため。
    Idle,
}

pub(crate) fn blur_grace_action(
    elapsed: Duration,
    focused: bool,
    auto_hide: bool,
    settings_running: bool,
) -> BlurAction {
    if blur_should_hide(focused, elapsed >= BLUR_GRACE, auto_hide, settings_running) {
        BlurAction::Hide
    } else if elapsed < BLUR_GRACE {
        BlurAction::Rearm(BLUR_GRACE - elapsed)
    } else {
        BlurAction::Idle
    }
}
```

```rust
// view.rs（:1321-1338 の置換）
if was_focused && !focused {
    self.unfocus_at = Some(Instant::now());
    ctx.request_repaint_after(crate::egui_shell::BLUR_GRACE);
}
if let Some(at) = self.unfocus_at {
    match crate::egui_shell::blur_grace_action(
        at.elapsed(),
        focused,
        self.auto_hide_enabled(),
        self.settings_running(),
    ) {
        BlurAction::Hide => {
            self.unfocus_at = None;
            self.emit_hide();
        }
        // 契約③: 予約は「フレーム 1 枚以上」しか約束しない。armed の間は毎フレーム
        // 残余を要求し直す（debounce・通知期限・起動タイムアウトと同じ流儀）。
        BlurAction::Rearm(remaining) => ctx.request_repaint_after(remaining),
        BlurAction::Idle => {}
    }
}
```

## 不変条件

1. **挙動不変**: `Hide` の条件は `blur_should_hide` そのままで、判定の意味は 1 文字も変わらない。増えるのは「猶予中フレームでの再予約」だけ
2. **`unfocus_at` の解除経路は 2 つのまま**（focus 復帰でクリア・Hide でクリア）。`Rearm` / `Idle` は状態を変えない
3. **`Idle` で再要求しない理由を型と doc に書く**——時間経過では状態が変わらないため（無条件 rearm にすると auto_hide off の間フレームを起こし続ける形になりうる）
4. **失敗・異常系**: 新規リソース・フラグ・スレッドなし。`Rearm` の残余は `elapsed < BLUR_GRACE` の分岐内でのみ算出するため減算のアンダーフローが構造的に起きない
5. **`BLUR_GRACE` の統合対象は view.rs の 2 箇所だけ**——`config_watcher.rs:63` の `from_millis(100)` は config 監視 debounce の別概念（統合しない）

## テスト方針

- **TDD（純粋核）**: `blur_grace_action` に `#[cfg(test)]` テストを先に書き Red を確認してから実装。ケース: 猶予中（0ms / 99ms）→ `Rearm(残余)` / 猶予明け + 全条件成立 → `Hide` / 猶予明けだが auto_hide off → `Idle` / 猶予明けだが settings 起動中 → `Idle` / focused=true（理論上・view はクリア済み）→ 猶予中なら `Rearm`・明けなら `Idle`
- 既存の `blur_should_hide` の意味を固定するテストは `lifecycle.rs` に無い（`plan_hotkey` のみ）——`blur_grace_action` のテストが実質その役も担う（命題の孤立は生じない）
- post-edit hook: clippy + `cargo test -p snotra`（沈黙=合格）
- **実機検証（カテゴリ C 相当）**: `npm run smoke:egui`（Escape → `egui_hide:done` の経路が生きていること）+ `cargo run -p snotra` で blur→hide の目視（ホットキーで表示 → 別ウィンドウをクリック → 100ms 程度で自動的に隠れる）

## SPEC.md 更新要否

不要。挙動不変であり、`SPEC.md` の blur 自動非表示の記述（猶予の存在・条件）は変わらない。契約③の記録先は設計 spec（既存）と `src-tauri/CLAUDE.md`（§8-5 の転記作業で扱う・本 issue のスコープ外）。

## コミット構成

1. `chore: workspace 調査・計画 (issue #711)`
2. `fix(egui): blur 猶予を armed 中の毎フレーム再要求へ揃える（契約③）(#711)`

## セルフレビュー

### 5a. plan-review の反映（偵察 1 + 独立導出 1）

- **偵察: 6 観点すべて問題なし**。`unfocus_at` の全 5 経路が計画の 3 箇所で尽きる／`blur_should_hide` の実消費は view.rs 1 箇所のみで 2 経路並走にならない／`Idle` 非再要求は**現行挙動と完全一致**／`auto_hide_enabled` `settings_running` の評価回数不変／`request_repaint_after` 全 6 箇所のうち blur が唯一の非対称、を実コードで裏付け
- **独立導出の致命指摘（採用・私の設計は既に回避済みと確認）**: 設計 spec §5 のスケッチは `at.elapsed()` を **3 回**読むため、判定（`>= grace`）と減算（`grace - at.elapsed()`）の間に時間が進むと `Duration` 減算が underflow → panic → release は `panic="abort"`（ルート Cargo.toml）で**プロセス abort**。**このフレームは 100ms 境界に着弾するよう予約されているため確率が低くない**。本計画は `elapsed: Duration` を**引数で 1 回だけ受け取り**、減算を `elapsed < BLUR_GRACE` の分岐内に閉じるため構造的に起きない（不変条件 4）。**この「1 回読み」は load-bearing であり、spec §5 へ errata を残す**
- **独立導出との一致（完全性の証拠）**: 再要求してよいのは「時間経過で解消する不成立」= `grace_elapsed == false` だけ／`armed` を `unfocus_at.is_some()` と取り違えると `request_repaint_after(ZERO)` の永久スピンになり #737 で潰した消費を別の扉から再導入する（→ 3 値 enum で構造的に固定する判断も一致）／`BLUR_GRACE` の統合対象は view.rs の 2 件のみ／`SPEC.md` 更新不要／`blur_should_hide` 無改修
- **採用した追加テスト**: `elapsed` が猶予を大きく超えた値（10s）でも underflow しないことの回帰テスト（上記罠の検出器）
- **スコープ外へ送る（受け皿を作る）**: 独立導出が発見した**既存の欠け**——`reset_pending` 消費ブロックが `launching` / `notice` / `search_debounce` をクリアするのに **`unfocus_at` / `was_focused` をクリアしない**（契約④「hide を跨ぐ時限状態は reset-on-show を backstop」に対する非整合）。猶予 armed のまま別経路で hide → 再 show の初フレームが `focused=false` なら即 auto-hide しうる。**現在は「show 後の初フレームを起こすのが `Focused(true)` 自身である」（spec §2.5）という性質に偶然守られている**——#671 PR A′ で踏んだ「意図されない保護」と同型。挙動変更を伴うため本 PR には含めず、**新規 issue を起票して名指しで送る**（#654 の教訓: 送り先を名指ししない deferred は脱落）
- **文書追随の分担**: 独立導出は `src-tauri/CLAUDE.md` / `snotra-egui-runtime/CLAUDE.md` への契約③の記載も推奨したが、**設計 spec §8 の 5 番（契約 5 か条の CLAUDE.md 転記）が担当する**ため本 PR では重複させない（spec §2 の表・§5 errata・進捗行のみ更新）

### 5a-2. Codex 敵対的レビュー（設計文書への読み取り専用レビュー・2026-07-26）

設計 spec の機構スケッチが 3 連続で後から欠陥を指摘されていたため、**残りにも同種の罠がある前提**で Codex に当てた。5 件（Critical 1 / High 2 / Medium 2）。

| # | 指摘 | 判定・反映 |
|---|---|---|
| 1 (Critical) | **契約③の本文が偽**——「d 経過後に少なくとも 1 枚来る」は、より早い要求に予約が吸収されると成立しない（`repaint.rs` の単一スロット + `take()`） | **採用**。私が独立に同定した「失われ方」と同じ機構だが、Codex は**契約本文そのものの誤り**として捉えた点が鋭い。契約③を全面改稿し errata を残した（規範は不変・根拠が差し替え） |
| 2 (High) | reset-on-show が `unfocus_at` / `was_focused` をクリアしない。**`set_focus()` は失敗を無視する**（`mod.rs`）ため初フレーム `focused=false` は実際に起こりうる | **採用（別 issue #745 へ反映）**。私の 2b 導出と同じ発見だが、Codex の「`let _ = set_focus()`」という証拠は masking がより脆いことを示す。#711 は挙動不変ゆえ含めず、#745 を強化 |
| 3 (High) | **`Idle` で再要求しない根拠に穴**——`settings_running` の true→false を起こす監視スレッドが wake しない（`window.rs`） | **採用。ただし是正は #746 へ**（下記） |
| 4 (Medium) | 契約②はリフレッシュレート低下直後の 1 回だけ下限を破る（gate が旧値で固定済み） | **採用（文書）**。コード側は既に「最大 1 dispatch の誤差・自己回復」とコメント済み。契約②へ限定を追記（全称表現の是正） |
| 5 (Medium) | §1 の経路列挙に「renderer 初期化失敗で waker が恒久 no-op」が無い | **採用（文書）**。§1 へ「抑止・終端経路」の表を追加（surface 初期化失敗・Destroyed・proxy 切断・hidden） |

**Codex が「健全」と確認した点（本 issue に効く）**: #711 の固定 timestamp からの re-arm は、猶予未満だけ `Rearm` し到達後は `Hide`/`Idle` へ抜けるため、**フレーム上限（契約②）との組み合わせでも無限ループも deadline ドリフトも生じない**。短い deadline は上限まで遅延するが、消費側は絶対経過時間を再判定するため耐える。

### settings 例外の扱い（ユーザー判断 2026-07-26・#746 へ分離）

Codex 指摘 3 を起点に、ユーザーが**より上流の問い**を立てた——「設定を開いたときメインウィンドウを隠すかは、そもそも `auto_hide_on_focus_lost` 設定次第ではないか」。調査の結果:

- **SPEC の状態機械は `focus_lost [auto_hide_on_focus_lost]` とガードを 1 つしか置いていない**（`SPEC.md`）。設定サイドカーについて SPEC が定めるのは `alwaysOnTop` の一時解除だけで、自動非表示の抑止には触れていない
- `!settings_running` は **SU2（`68b5f41`）で根拠の記録なく入った**。設計 spec に決定として残っておらず、**WebView2 期に前例も無い**（`git log -S` が 0 件）

→ **ユーザー判断: ガードを外して SPEC に揃える。挙動変更ゆえ #746 として分離**（実機確認 3 件とセット）。

**#711 への帰結**: `wake_main` の追加は行わない——#746 でガードが外れれば、auto_hide 有効時は設定を開いた時点で main が隠れるため、**「設定終了時に猶予が宙吊りになる」経路そのものが消える**。今日の穴は**既存**であり #711 が悪化させるものではないが、`Idle` の根拠に例外が残る間は**それを doc に正直に書く**（設計 spec §5 の errata が #746 を名指しする）。

**Codex が「未確認」と正直に述べた点**: 契約⑤の「すべての連続アニメーションが有限時間で収束する」は egui 内部の状態機械まで要り、リポジトリ内の引用では確認できない → **全称の保証を取り下げ、#710 の実測（482 バーストすべて終端）に限定する**形へ改稿した。

### 5b. plan-review が扱わない 3 観点

1. **境界条件**: `elapsed == BLUR_GRACE` ちょうど（`>=` ゆえ Hide 側・`Rearm` に落ちず減算も起きない）／`elapsed = 0`（`Rearm(100ms)`）／`elapsed` が猶予を大きく超過（`Idle` または `Hide`・減算しない）／`focused=true` で armed（view は先にクリアするため到達しないが、純粋核としては猶予中 `Rearm`・明け `Idle` で無害）
2. **シンプル化**: 新規状態・フラグ・スレッドなし。const 1 個 + enum 1 個 + 純関数 1 個が最小形。`blur_should_hide` を内部再利用し判定の二重定義を作らない。「この操作が失敗したら」= 失敗経路が無い（純関数・減算は分岐で保護）
3. **破壊不変条件 + 検知手段**: ①「hide が宙吊りにならない」→ `Rearm` のユニットテスト + 実機で blur→自動 hide の目視 ②「永久スピンを作らない」→ `Idle` のユニットテスト（猶予明け + auto_hide off で `Rearm` を返さないこと）+ 実機で auto_hide off・blur 放置時の CPU 目視 ③「underflow で abort しない」→ 大きな `elapsed` の回帰テスト ④「通常の blur→hide が退行しない」→ `npm run smoke:egui`（Escape→`egui_hide:done`）+ 目視

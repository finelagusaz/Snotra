# plan — issue #456: 破棄押下フレームの footer 赤枠（id 不安定性）

## 前提（research.md の確定事項）

- 真因 = **`warn_if_rect_changes_id`**（id 不安定性、赤幅2.0・🔥 なし）。issue の `check_for_id_clash`
  （ID 重複）診断は**誤り**。
- 根本原因 = footer の status ラベル条件分岐が外側 ui の auto-id カウンタをずらし、RTL ボタン群
  （`IdSource::Child`）の配下ボタン id を変化させる（矩形は不変）。
- **修正・検証まで headless で完了済み**。本 plan は実装（/implement）で「診断 → 回帰テスト固定化 →
  修正適用 → 検証」を Red→Green で再実行するための as-built 手順。

## 変更ファイル一覧

| ファイル | 変更 |
|---|---|
| `snotra-settings/src/app.rs` | ① footer ボタン群を明示 id 化（修正本体） ② 回帰テスト追加 |
| `snotra-settings/CLAUDE.md` | 「egui 実装の注意点」に id 不安定性の知見を追記 |

SPEC.md: **更新不要**（debug 描画警告の解消のみ。フロー・IPC・状態遷移・設定キー・データ形式いずれも不変）。

## Phase 1 — 回帰テスト追加（Red）

`app.rs` の `#[cfg(test)] mod tests` に追加。`error_fg_color`(255,0,0) の rect_stroke を
`warn_if_rect_changes_id` のマーカーとして、discard の press→release 各フレームを走査し**0件**を検証する。
修正前は release 直後フレームで4件出て**落ちる**ことを確認する（Red）。

```rust
// 不変条件: 破棄クリックの遷移フレームで egui の id 不安定性警告
// （warn_if_rect_changes_id = 赤枠, debug 限定）が出ないこと（#456）。
// status ラベルの出現/消失が footer ボタン群の auto-id をずらさないことを保証する。
#[test]
fn kittest_discard_no_rect_id_instability_warning() {
    // error_fg_color(255,0,0) の rect_stroke = egui の id 不安定性/clash 警告マーカー
    fn red_error_rects(harness: &Harness<'static, SettingsApp>) -> usize {
        harness
            .output()
            .shapes
            .iter()
            .filter(|cs| {
                matches!(&cs.shape, egui::Shape::Rect(r)
                    if r.stroke.color == egui::Color32::RED)
            })
            .count()
    }

    let mut h = en_harness(en_config());
    h.set_size(egui::vec2(760.0, 560.0));
    // dirty 化（status ラベルが出る = 前置ウィジェットが増える）
    h.get_by_label(Tr(Language::En).cb_hotkey_toggle()).click();
    settle(&mut h);
    assert!(h.state().has_changes());

    let center = h.get_by_label(Tr(Language::En).btn_discard()).rect().center();

    // press→release を _step 単位で送り、各フレームで警告矩形が出ないことを確認する。
    // （.click() は press+release を 1 step 内で処理し過渡フレームを潰すため、個別に送る）
    let press = |pressed: bool| egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut steps: Vec<egui::Event> = vec![egui::Event::PointerMoved(center), press(true)];
    // hold / release / 後続フレームも個別に回す
    for (i, ev) in steps.drain(..).enumerate() {
        h.event(ev);
        h.step();
        assert_eq!(red_error_rects(&h), 0, "id 警告矩形が press 系フレーム{i}で出た");
    }
    h.step(); // hold
    assert_eq!(red_error_rects(&h), 0, "hold フレームで警告矩形");
    h.event(press(false));
    h.step(); // release
    assert_eq!(red_error_rects(&h), 0, "release フレームで警告矩形");
    h.step(); // 遷移直後（修正前はここで赤枠4件）
    assert_eq!(red_error_rects(&h), 0, "discard 遷移フレームで id 不安定性警告が出た（#456 回帰）");
    h.step();
    assert_eq!(red_error_rects(&h), 0, "遷移+1 フレームで警告矩形");
    assert!(!h.state().has_changes(), "discard で dirty 解消しているはず");
}
```

注意点:
- `egui::Color32::RED` == `from_rgb(255,0,0)` == `error_fg_color`。`warn_on_id_clash`（clash）と
  `warn_if_rect_changes_id`（id 変化）の両方がこの色を使うため、どちらの回帰も検出できる。
- `settle` / `en_harness` / `en_config` は既存ヘルパを流用。
- テストが依存する `warn_if_rect_changes_id` は debug（`cfg!(debug_assertions)`）で有効 = テスト実行時 true。

## Phase 2 — 修正適用（Green）

`app.rs:488` を置換:

```rust
// before
ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
    ...
});

// after — 明示 id でボタン群の auto-id を status ラベルの有無から独立させる。
// with_layout は scope_builder(UiBuilder::new().layout(layout), …) の薄いラッパ
// （egui ui.rs）なので、.id() を足すだけでレイアウト/挙動は不変。
// IdSource::Child(push_id/id_salt) は unique_id に親カウンタを混ぜるため id 不安定を
// 解消できない。IdSource::Explicit(=UiBuilder::id) のみがカウンタ混入を断つ（#456）。
ui.scope_builder(
    egui::UiBuilder::new()
        .id(egui::Id::new("footer_actions"))
        .layout(egui::Layout::right_to_left(egui::Align::Center)),
    |ui| {
        ...
    },
);
```

適用後、Phase 1 のテストが green になることを確認する（検証済み: 修正前赤枠4→修正後0）。

## Phase 3 — ドキュメント追記

`snotra-settings/CLAUDE.md` の「egui 実装の注意点」に:

> - **条件付きの前置ウィジェットは後続の `with_layout`/auto-id ウィジェットの id を不安定にする**:
>   footer の status ラベルのように「状態で出現/消失する」ウィジェットを、矩形固定（RTL 右寄せ等）の
>   ウィジェット群の**前**に置くと、後続の auto-id がフレーム間で変化し egui の `warn_if_rect_changes_id`
>   （debug 限定・赤枠2px・🔥 なし。`check_for_id_clash` の ID 重複とは別物）が発火する。
>   `push_id`/`id_salt`（`IdSource::Child`）は `unique_id = stable_id.with(next_auto_id_salt)` で
>   親カウンタを混ぜるため解消できない。**`UiBuilder::new().id(Id::new(...))`（`IdSource::Explicit`）**で
>   コンテナに明示 id を与えて配下 auto-id を安定化する（#456）。

## 検証

- **Red→Green（必須・実施済み）**: Phase 1 テストが修正前 fail・修正後 pass。
- `cargo test -p snotra-settings`（既存 32 件 + 新規 1 件、全 pass。footer wiring 無影響を確認済み）。
- clippy（PostToolUse フックが app.rs 編集で自動実行）。
- 視覚スモーク（補助）: `cargo run -p snotra-settings`（debug）で破棄押下時に赤枠が出ないことを目視。

## 不変条件

- **レイアウト/描画/挙動不変**: `with_layout`→`scope_builder(.id())` は id 名前空間のみ変更。ボタン配置・
  Tab 順（sentinel 経路）・クリック検出は不変（既存 kittest 4 件が保証）。
- **明示 id `"footer_actions"` は一意**: 他ウィジェットと衝突しない（コード全体で未使用）。footer は
  毎フレーム1回のみ描画 → 同一パス内の二重登録も起きない（`check_for_id_clash` を新たに誘発しない）。
- **`saved` は Save 成功時のみ更新**（既存不変条件・本修正は触れない）。
- **debug/release 共通**: 修正は `cfg!(debug_assertions)` に依存しない（id を安定化するだけ。警告のゲートは
  egui 側）。release でも id 安定化の恩恵（フォーカス管理の正しさ）を受ける。

## セルフレビュー

1. **対称コードパス**: Save/Discard/Reset は同一コンテナ内の3ボタン。明示 id 化は3ボタンをまとめて包むため
   対称に適用。footer は Backup タブで非表示（`if self.active_tab != TabId::Backup`）だが、その分岐は不変。
2. **影響範囲**: footer は `Panel::bottom("footer")` 1箇所のみ。他タブ・sidebar・CentralPanel に波及なし
   （grep 済み）。`scope_builder` は egui 標準 API。
3. **境界条件**: dirty→clean（discard/save）だけでなく clean→dirty（初回編集）でも status ラベルが
   トグルする。回帰テストは discard 経路を張るが、修正（明示 id）はトグル方向に依存せず両方向を安定化する。
4. **リソース管理**: 生成/破棄ペアなし（scope は自動 drop）。
5. **既存パターン整合**: 明示 id は egui 標準。他所の `from_id_salt`/`Id::new`（ComboBox/Modal）と同流儀。
6. **YAGNI**: 修正は1箇所。汎用機構を足さない。
7. **シンプル化**: 代替（status ラベルを常時確保 / ボタンを先に描く）はレイアウト副作用があり、明示 id が
   最小・無副作用。新状態・Mutex・プロセス導入なし。
8. **破壊不変条件**: Win32 フック・ホットキー・IPC に触れない。検知手段は Phase 1 回帰テスト（headless・
   決定的）+ 視覚スモーク。

## check スキル結果（Step 5a）

- **`/plan-review`（実施済み）**: サブエージェント（Explore）が当初 plan の**結論3を反証**——
  「kittest では active-press を再現できない」は egui_kittest node.rs:53-71 上の事実誤認（`.click()` は
  座標 press/release で active 状態を通過）で、非再現の真因は「過渡フレームの走査漏れ（`self.output` の
  上書き）」と指摘。これを受けフレーム単位走査で**再現に成功**し、真因（`warn_if_rect_changes_id`）と
  修正（明示 id）を確定・検証した。当初 plan（「実バイナリで ID 手動特定」ゲート）は**破棄**。
- `/state-check` `/race-check` `/cache-check` `/persistence-check` `/symmetric-check`: **非該当**
  （UI モード/ガード・async・キャッシュ・on-disk 形式・show/hide 対称ペアに触れない。id 派生の安定化のみ）。

# research — issue #456: 破棄押下フレームで footer に赤枠が1フレーム出る（debug 限定）

## 結論（先出し）

- **issue の診断は誤り**: 赤枠は `Context::check_for_id_clash`（ウィジェット **ID 重複** / 🔥 "Double use"）**ではない**。
  真因は egui の別の debug チェック **`warn_if_rect_changes_id`**（context.rs:4177）——「同じ矩形位置のウィジェットが
  **パス間で異なる id** を持った」= **id 不安定性**の警告（赤 stroke 幅2.0・**🔥 テキストなし**）。
- **根本原因**: footer の status ラベル（「未保存の変更があります」）が `has_changes()` の切り替わりで
  出現/消失し、外側 ui の auto-id カウンタをずらす。RTL（右寄せ）のボタン群は**矩形位置が不変**なのに
  **auto-id が変化**するため、egui が「rect changed id between passes」と警告する。
- **debug 限定**: `warn_if_rect_changes_id: cfg!(debug_assertions)`（style.rs:1398）。release では無効
  → issue の A/B 切り分け（現行/#452前/0.34.3 いずれも debug で出る・既存問題）と完全一致。
- **修正（検証済み）**: footer のボタン群コンテナに**明示 id**を与える（`UiBuilder::new().id(Id::new("footer_actions"))`）。
  headless で赤枠の消失を実測確認済み（Red→Green）。

## issue の要約

snotra-settings（debug ビルド）で値を変更し「破棄」を押すと、押下→dirty 解消のフレームで footer の
ボタン周辺に赤い枠線が1フレーム出る。issue はこれを egui の widget ID 重複警告（`check_for_id_clash`）と
推定し「🔥 が示す ID を特定 → push_id で一意化」を次の一手としていた。**本調査で診断を訂正した。**

## 実証プロセス（egui_kittest による headless 再現）

`app.rs` の kittest 基盤（`Harness::new_ui_state` + `ui_impl`）で診断テストを一時追加・実行（**特定後に削除済み**）。

### 検出手法の妥当性（陽性コントロール）
意図的な同一 id 二重登録を kittest で回すと `output().shapes` に `🔥 First/Second use of widget ID …` が
確実に現れた。→ `output().shapes` 走査は egui の debug 描画を捕捉できる。

### 再現の鍵 — フレーム単位走査
`.click()` は `hover→PointerButton(press)→PointerButton(release)` を**実座標**で発火する
（egui_kittest node.rs:53-71）。かつ kittest の `step()` は queue した各イベントを**別々の `_step`** で処理し、
`self.output` は毎 `_step` で上書きされる。当初 `harness.step()`（内部で press+release の2 `_step`）**後**にしか
走査せず、過渡フレームの出力を取りこぼしていた（`.click()` は AccessKit アクションで active 状態を回避する、
という当初の推測は node.rs 上**誤り**——`.click()` は座標 press/release で active 状態を通過する）。
**イベントを1つずつ送り `_step` 単位で走査**したところ、release 直後（dirty=false 化）フレームで再現した。

### 捕捉した描画（AFTER1 フレーム）
release 直後フレームに、透明 fill・**赤(255,0,0)・幅2.0**の rect_stroke が**4つ**（🔥 テキストは0件）:
- `[489,512]-[740,552]` = ボタン群コンテナ（RTL 子 ui）の矩形
- `[489,520]-[606,543]` / `[614,520]-[681,543]` / `[689,520]-[740,543]` = Reset / Discard / Save 各ボタン

### 描画源の特定（egui 一次ソース）
| 事実 | 根拠 |
|---|---|
| `check_for_id_clash` は幅**1.0** + 必ず 🔥 debug_text を描く | context.rs:1119-1166 |
| `warn_on_id_clash=false` にしても赤枠は**消えない** | 実測（= clash 機構とは無関係） |
| 赤幅2.0 の唯一の源は `warn_if_rect_changes_id` の `rect_stroke(rect, 0, (2.0, Color32::RED), Outside)` | context.rs:4266-4272 |
| 同関数は `log::warn!("Widget rect … changed id between passes: prev ids …, new ids …")` も出す | context.rs:4254 |
| `prev_pass.widgets` vs `this_pass.widgets` を比較（前パス vs 今パス） | context.rs:2632-2638 |
| gate は `debug.warn_if_rect_changes_id = cfg!(debug_assertions)`（debug 既定 true） | style.rs:1370,1398 |

## 根本原因のメカニズム（egui の id 導出）

`ui.rs:208-277` `new_child` の id 導出:
```
IdSource::Child(id_salt) => stable_id = parent.id.with(id_salt);
                            unique_id = stable_id.with(parent.next_auto_id_salt);  // ← 親カウンタ混入
child.id            = stable_id                       // カウンタ非依存
child.unique_id     = unique_id                       // カウンタ依存（widget rect 登録に使う）
child.next_auto_id_salt = unique_id.value()+1         // カウンタ依存（配下ボタンの auto-id 種）
IdSource::Explicit(id) => (id, id)                    // カウンタ非依存（混入なし）
```

footer の構造（app.rs:472-519）:
```
ui.horizontal_centered(|ui| {
    if status_text.is_some() { ui.label(text); ui.separator(); }   // 条件付き = 前置ウィジェット数が可変
    if not Backup { ui.with_layout(RTL, |ui| { Save; Discard; Reset }); }  // = scope_builder(Child id)
});
```
- discard で `has_changes()` が true→false になると status ラベル+separator が**消える**。
- これで外側 ui の `next_auto_id_salt` が変わり、`with_layout`（= `IdSource::Child`）が作る RTL コンテナの
  `unique_id`・`next_auto_id_salt` に流入 → **配下ボタンの auto-id も変化**。
- RTL は右寄せなのでボタンの**矩形は不変** → 「同矩形・別 id」→ `warn_if_rect_changes_id` 発火。

**`push_id`（= `IdSource::Child`）では直らない**ことを実測確認した——`unique_id = stable_id.with(next_auto_id_salt)`
が依然カウンタを混ぜるため。**`IdSource::Explicit`（`UiBuilder::id`）のみ**がカウンタ混入を断ち、配下ボタンの
id 種（`next_auto_id_salt = id.value()+1`）を安定化する。

## 修正（headless 検証済み）

`app.rs:488` の
```rust
ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ... });
```
を
```rust
ui.scope_builder(
    egui::UiBuilder::new()
        .id(egui::Id::new("footer_actions"))
        .layout(egui::Layout::right_to_left(egui::Align::Center)),
    |ui| { ... },
);
```
に置換。**実測**: 修正前=赤枠4つ、修正後=`any clash detected: false`（赤枠0）。既存テスト 32 件すべて pass
（footer wiring 無影響）。`with_layout` は元々 `scope_builder(UiBuilder::new().layout(layout), …)` の薄いラッパなので
（ui.rs:2469-2475）、`.id()` を足すだけで挙動不変・レイアウト不変。

## 関連コード

- `snotra-settings/src/app.rs:468-520` — footer（`Panel::bottom("footer")`）。status ラベル条件分岐 + RTL ボタン群。
- egui 0.35 一次ソース（`~/.cargo/registry/src/.../egui-0.35.0/src/`）:
  - `context.rs:4177` `warn_if_rect_changes_id`、`:2632` 呼び出し、`:1097` `check_for_id_clash`（別物）
  - `ui.rs:208` `new_child`（id 導出）、`:2163` `push_id`、`:2469` `with_layout`、`:2193` `scope_builder`
  - `ui_builder.rs:56` `id_salt`(Child)、`:71` `id`(Explicit)
  - `style.rs:1370,1398` `warn_if_rect_changes_id` 既定値

## 技術的制約

- **debug 限定**（`cfg!(debug_assertions)`）ゆえ release ユーザーには不可視。実害は「フォーカス/操作の火種に
  なりうる id 不安定性」で、id を安定化すれば根本解消。
- 検証は **egui_kittest で headless 再現可能**（当初「不可能」と評価したが誤りだった。フレーム単位走査 +
  `error_fg_color`(255,0,0) の rect_stroke 検出が確実なマーカー）。視覚スモークは補助（実機 debug で目視）。
- Win32 依存なし（純 egui 層）。

## 未解決の疑問

なし（真因・修正・検証まで確定）。残タスクは「診断を回帰テストとして固定化」（plan.md 参照）と、
知見（`warn_if_rect_changes_id` ≠ `check_for_id_clash`、`IdSource::Child` はカウンタ混入）の
`snotra-settings/CLAUDE.md` への追記。

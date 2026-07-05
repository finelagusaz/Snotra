# research — #440 egui 設定 GUI のヘッドレス UI テスト導入検討

## issue の要約

設定 GUI（`snotra-settings`, egui）は「型検査 + clippy + 人手の視覚スモーク」だけで守られており、コードベース最大の検証死角。egui はネイティブ描画のため WebDriver ベース E2E から原理的に不可視。egui 公式のヘッドレステストハーネス `egui_kittest`（AccessKit 経由の操作 + スナップショット比較）を評価し、以下を判断する:

1. draft/saved フロー・バリデーションの**操作テスト**が書けるか（AccessKit 経由）
2. **スナップショット比較**で #399 型の視覚回帰を検知できるか（フォントレンダリングの環境差にどう耐えるか）
3. **CI**（GitHub Actions windows ランナー）で安定して回るか

導入コストが見合わない場合の代替（起動スモーク + タブのスクリーンショット保存）も比較する。

## 確定した事実（実証・一次情報）

### egui_kittest 0.35 は存在し、eframe 統合機能を持つ

`cargo add egui_kittest@0.35 --dev -p snotra-settings --dry-run` の実行結果:

```
Adding egui_kittest v0.35 to dev-dependencies
Features as of v0.35.0:
- document-features
- eframe      ← eframe::App を直接テストする統合
- snapshot    ← 画像スナップショット比較（wgpu 必須）
- wgpu        ← GPU レンダラ（スナップショットに必要）
- x11
```

- eframe 0.35 と**バージョン整合**（egui_kittest は egui/eframe とロックステップでリリース）。本 crate は `eframe = "0.35"` なので追加互換性リスクは低い。
- `eframe` feature があり、`SettingsApp`（`impl eframe::App`）をそのまま Harness に載せる経路がありうる（詳細は要スパイク）。

### API 形状が `SettingsApp::ui()` とほぼ一致（切り出しが薄い）

context7（`/websites/crates_io_crates_egui_kittest`）の README:

```rust
let app = |ui: &mut egui::Ui| { ui.checkbox(&mut checked, "Check me!"); };
let mut harness = Harness::new_ui(app);
let checkbox = harness.get_by_label("Check me!");
checkbox.click();
harness.run();
assert_eq!(harness.get_by_label("Check me!").accesskit_node().toggled(), Some(Toggled::True));
harness.fit_contents();
#[cfg(all(feature = "wgpu", feature = "snapshot"))]
harness.snapshot("readme_example");   // 画像スナップショットは wgpu+snapshot 限定
```

- `Harness::new_ui(|ui: &mut egui::Ui| …)` は**現行の `SettingsApp::ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame)`（`app.rs:275`）とシグネチャがほぼ一致**。`_frame` は未使用（アンダースコア）なので、`fn ui_impl(&mut self, ui: &mut egui::Ui)` を切り出して両方（eframe の `ui()` と kittest Harness）から呼べば、egui 描画コードを一切書き換えずにヘッドレステスト可能。
- 操作は AccessKit ツリー経由（`get_by_label` / `.click()` / `.run()`）。**wgpu 不要**。

### スナップショットの環境差リスク（context7 一次情報）

egui_kittest 公式ドキュメント「What to do when CI / another computer produces a different image?」が明示:

- 画像差は **GPU / OS / レンダリングバックエンド（Metal/Vulkan/DX12）/ グラフィックドライバ**依存。MSAA のサンプル配置・テクスチャフィルタ・浮動小数点評価・偏微分（dpdx）の実装差で発生。
- 対策は「全 test run で feature を統一」「`SnapshotOptions::threshold` と許容ピクセル数を調整」だが、**「許容度を上げすぎると本物の失敗をマスクする」と警告**。

## テスト可能性の分析（コード側の制約）

### 操作テスト（AccessKit）— 実現可能・障壁小

`SettingsApp` の draft/saved 二重状態モデルは純ロジック（`has_changes()` = `draft != saved`, `save()`, `reset_to_default()`, `SECTION_TABLE` ダーティ点導出）。AccessKit 経由で「入力ウィジェットを操作 → 内部状態を assert」が書ける。

検証死角として issue が挙げた項目のうち **AccessKit で書けるもの**:
- draft 編集 → `has_changes()` true → ウィンドウタイトル `*` 付与
- Discard クリック → `draft = saved` で編集破棄
- Reset to default → `draft = normalized_default()`
- バリデーションエラー時のステータス表示（`save()` は `validate()` を disk 書き込み前に呼ぶ = **バリデーション経路は disk に触れずテスト可能**）
- タブ別ダーティ点（`•`）の導出
- モーダル Create/Edit 状態機械（`tabs/common.rs`）

### 障壁: `new()` の Win32 依存と `save()` の disk 書き込み

- `SettingsApp::new()` は `crate::font::list_system_fonts()`（Win32）を呼ぶ（`app.rs:173`）。ヘッドレス CI では列挙結果が dev 機と異なる/空になりうる。テスト用に **font_list を注入する `new_for_test(config, font_list)` 経路**か、`list_system_fonts` を DI 可能にする必要がある。
- `save()` の成功経路は `Config::save()` で**実 `config.toml` に書き込む**（`app.rs:208`）。Save クリックの成功をテストするには temp dir 注入が要る。ただし **disk 書き込み後の状態遷移（`saved = draft`）は snotra-core 側の `Config::save`/`load` テストで既にカバー**されており、settings 側の操作テストは「Save 前のバリデーション分岐」「Discard」「dirty 判定」に絞れば disk 注入なしで成立する。

### スナップショットテスト — 実現可能だが CI 安定性に本質的リスク

- `snapshot` + `wgpu` feature が要る。Windows GH Actions ランナーで wgpu を回すには DX12 か software adapter（WARP）が要り、**フォントレンダリングが system font（Yu Gothic UI / Segoe UI）依存**。日本語見出しを含む本 UI のスナップショットは dev 機と CI で確実にピクセル差が出る。
- 環境差を吸収する高い threshold は、issue が検知したい **#399 型欠陥（混在見出しのサブピクセル・ベースラインずれ）そのものをマスク**する。つまりスナップショットは「守りたい欠陥クラス」と「吸収したいノイズ」が同じ粒度に居るため、この特定欠陥に対する ROI が低い。

## 代替案（fallback）との比較

| 手段 | 検知できる欠陥 | CI 安定性 | コスト | #399 型検知 |
|------|--------------|----------|--------|-----------|
| AccessKit 操作テスト | 状態遷移・バリデーション・ダーティ・モーダルの**ロジック回帰** | ◎ 決定的・GPU 不要 | 小（`ui_impl` 切り出し + テスト） | ✗（レイアウトは見ない） |
| wgpu スナップショット | レイアウト・レンダリング回帰（理論上） | △ GPU/フォント/driver 依存で flaky | 中（feature + threshold 調整の継続コスト） | △ 高 threshold がマスクする |
| 起動スモーク + スクショ保存（fallback） | クラッシュ・パニック（起動時）+ **人手レビュー補助** | ◎ assert しない artifact 保存なら安定 | 小（release smoke #441 の延長） | ✗ 自動検知はしない（目視補助のみ） |

## 未解決の疑問（スパイクで確定させる = /implement Phase 1）

1. `egui_kittest` の `eframe` feature で `SettingsApp`（`impl eframe::App`）を直接載せられるか、それとも `Harness::new_ui(ui_impl)` の方が薄いか。→ 両方試して薄い方を採る。
2. `list_system_fonts()` のヘッドレス挙動（空 Vec を返すか panic か）。→ 空でも Harness が回るなら注入不要かもしれない。
3. GH Actions windows ランナーで `cargo test -p snotra-settings`（wgpu **なし**、AccessKit のみ）が追加インフラなしで green になるか。

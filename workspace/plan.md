# plan — #440 egui ヘッドレス UI テスト導入

## 結論（推奨パス）

research の実証に基づく判断:

- **採用**: `egui_kittest` の **AccessKit 操作テスト**（wgpu なし）。決定的・GPU 不要・CI 安定。設定 GUI の状態遷移／バリデーション／ダーティ判定／モーダル状態機械という「ロジック検証死角」を自動化する。
- **非採用（CI ゲート）**: **wgpu スナップショット**。フォント/GPU/driver 依存で flaky、かつ環境差吸収 threshold が #399 型欠陥をマスクするため ROI が低い。将来ローカル opt-in の余地は残すが CI では回さない。
- **任意（低優先・別 issue 候補）**: 起動時スクショ artifact 保存。assert しない目視補助。release smoke（#441）の延長で安価だが、本 issue の必須スコープには含めない。

この plan は **Phase 1（スパイク = 実証）を最初に置き、そこで採用是非を最終確定**する。スパイクが green なら Phase 2〜3 に進み、赤なら research の未解決点を反映して再判断する。

## 変更ファイル一覧

| ファイル | 変更内容 | Phase |
|---------|---------|-------|
| `snotra-settings/Cargo.toml` | `[dev-dependencies] egui_kittest = { version = "0.35", default-features = false }`（wgpu/snapshot feature は付けない） | 1 |
| `snotra-settings/src/app.rs` | `SettingsApp::ui()` から `fn ui_impl(&mut self, ui: &mut egui::Ui)` を切り出し、eframe の `ui()` はそれを呼ぶだけにする。テスト用コンストラクタ `new_for_test(config, font_list)`（または `list_system_fonts` の DI）を追加 | 1 |
| `snotra-settings/tests/settings_ui.rs`（新規） | egui_kittest による AccessKit 操作テスト（下記シナリオ） | 2 |
| `snotra-settings/CLAUDE.md` | 「ユニットテストは書かない方針」の**例外拡張**を明文化（egui_kittest による操作テストは書ける／スナップショットは CI で回さない理由）。モジュール構成に `tests/` 追記 | 3 |
| `.github/workflows/ci.yml` | rust-check の `cargo test (snotra-settings)` が新テストを含むことを確認（既に #444 で test ステップ追加済み。feature 追加なしなら**変更不要**の可能性大 → 実測で確定） | 3 |
| `docs/build-commands.md` | 検証チェックリストに影響あれば同期（テストコマンドは既存の `cargo test -p snotra-settings` で足りる見込み → 差分が出たときのみ） | 3 |

## 実装順序（フェーズ分け）

### Phase 1 — スパイク（実証で採用是非を確定）
1. `egui_kittest = "0.35"`（wgpu/snapshot なし）を dev-dependency に追加。
2. `ui_impl` 切り出し + `new_for_test` 追加。`cargo build -p snotra-settings` green を確認。
3. **最小テスト 1 本**（例: General タブでホットキー入力を変える → `harness` 経由で `has_changes()` 相当が true）を書いて `cargo test -p snotra-settings` が**ローカルで green** かつ **CI windows で green** になることを確認（この 1 点が採用可否の決め手＝実証）。
   - `list_system_fonts()` がヘッドレスで空 Vec を返すだけなら DI 不要。panic するなら `new_for_test` で注入。→ 実測で分岐。
4. **ゲート**: Phase 1 が赤（wgpu 抜きでも AccessKit が回らない / Harness に載らない）なら、fallback（スクショ artifact）へ切替を再検討し plan を更新してからユーザーに報告。

### Phase 2 — 操作テスト拡充（Phase 1 green 前提）
draft/saved モデルと検証死角に対応するシナリオを追加:
- **dirty flow**: draft 編集 → `has_changes()` true → タイトル `*`
- **Discard**: 編集後 Discard → `draft == saved`
- **Reset to default**: → `draft == Config::normalized_default()`、`has_changes()` true
- **バリデーションエラー**: 不正入力（例: window width を最小未満）で Save → ステータスにエラー、`saved` 不変（disk 非依存 = `validate()` 分岐）
- **タブ別ダーティ点**: あるセクションを編集 → 対応タブのみ `•`
- **モーダル状態機械**: index/opener/instant いずれかで Create → Save で Vec に push、Edit → 上書き、Delete（`tabs/common.rs` の境界チェック込み）

各テストは AccessKit ノード操作 + 内部状態 assert。disk 書き込みを伴う Save 成功経路は snotra-core 側テストに委ね、settings 側は「Save 前分岐」「Discard」「dirty」に限定する。

### Phase 3 — ドキュメント同期
- `snotra-settings/CLAUDE.md`: 「ユニットテスト書かない方針」の例外に「egui_kittest 操作テスト（AccessKit）は書く。wgpu スナップショットは環境差 flaky + #399 型欠陥をマスクするため CI 非採用」を追記。`tests/settings_ui.rs` をモジュール構成へ。
- 視覚回帰（#399 型・レイアウト崩れ）は依然として**人手視覚スモークが唯一の検知手段**である旨を維持（メモリ `feedback_codex_review_unreliable` の境界と整合）。
- `docs/build-commands.md` / `ci.yml` は実測差分が出たときのみ同期。

## 不変条件

- **egui 描画コードを書き換えない**: `ui_impl` 切り出しは純粋な機械的抽出（`_frame` 未使用のため挙動不変）。eframe の `ui()` は `ui_impl` への 1 行委譲になる。切り出し前後で `cargo run -p snotra-settings` の視覚スモークが同一であることを目視確認する（レイアウト崩れ観点）。
- **テスト専用経路が本番挙動を変えない**: `new_for_test` / font_list DI は `#[cfg(test)]` もしくはテスト専用引数に閉じ、`run()` の本番パス（`list_system_fonts()` 呼び出し）を変えない。
- **CI に GPU 依存を持ち込まない**: dev-dependency は `default-features = false` で wgpu/snapshot を**引き込まない**。これにより既存 CI（windows ランナー、GPU なし）がそのまま回る。feature 追加は将来 opt-in の別判断。
- **失敗時の挙動**: Phase 1 スパイクが CI で不安定なら、feature/インフラを増やす前に fallback へ退避（本 plan の Phase 1 ゲート）。回復不能な状態フラグ・プロセス・Win32 フックは導入しない（純テストコードのみ）。

## テスト方針

- 追加テスト: `snotra-settings/tests/settings_ui.rs`（egui_kittest）。検証する不変条件は各シナリオ（dirty / discard / reset / validation / dirty-dot / modal）。
- 検証コマンド: `cargo test -p snotra-settings`（category A）、`cargo clippy -p snotra-settings`（PostToolUse フック自動）、`cargo run -p snotra-settings` 視覚スモーク（`ui_impl` 切り出しの挙動不変確認）。
- Red→Green: Phase 1 の最小テストは、まず AccessKit で「操作 → 状態変化」を書き、切り出し前は Harness に載らない（コンパイル不可）= Red、`ui_impl` 追加で Green。

## SPEC.md 更新要否

**不要**。挙動変更なし（テストインフラ追加 + 機械的リファクタのみ）。IPC 契約・状態遷移・フローに変更なし。`ui_impl` 切り出しは実装事実の内部整理で、SPEC の意図に影響しない。

## as-built（実装後の差分記録）

Phase 1 スパイクの実証で計画から変わった点:

- **テストは `tests/settings_ui.rs` でなくインライン `app.rs #[cfg(test)] mod tests`**: `SettingsApp`/`new`/`has_changes` が private のため integration test から不可視。内部状態 assert にはインラインが必須。
- **font DI 不要**: `list_system_fonts()`（Win32 GDI）はヘッドレスでも動作。`new_for_test` は追加せず `new()` をそのまま使用（計画からさらに単純化）。
- **`Harness::run()` でなく `settle`（固定 step）**: UI が checkbox アニメで毎フレーム repaint 要求 → `run()` は収束前提で panic。`step()` を数回。
- **実装したテスト**: `kittest_checkbox_click_makes_draft_dirty` / `kittest_discard_reverts_draft_to_saved` / `kittest_reset_to_default_makes_dirty` / `kittest_save_with_validation_error_keeps_saved`（フッターボタン wiring + save→validate 経路。dirty-dot/modal は純ロジックテスト済みのため重複回避）。
- **検証結果**: `cargo test -p snotra-settings` 32 passed / `cargo clippy --all-targets -D warnings` クリーン。

## セルフレビュー

### `/plan-review`（Explore 2体: Rust feasibility / CI・docs 同期）+ 主エージェント直接確認

**要対処: なし。** Step 2b（独立再導出）は本計画が局所的（単一 crate・rename/config キー/移行/横断スイープなし）のためトリガー外 = 省略。

**問題なし（一次資料で確認）**
- `_frame` は `app.rs:275` のシグネチャのみに出現（他の一致はコメント `id_next_frame`）→ `ui_impl` 切り出しは**挙動不変**。
- `SettingsApp::ui` の直接呼び出し元は src に無し（eframe 内部のみ）。タブの `ui` は別 free 関数で無関係。`SettingsApp::new` の呼び出し元は `run()`（`app.rs:619`）のみ → `new_for_test` 追加は本番パス不変。
- `save()` は `validate()`（`app.rs:198`）を `config.save()`（`app.rs:208`, disk 書き込み）**前に**呼ぶ → バリデーションテストは disk 非依存で成立。
- `egui_kittest 0.35` は `[features]` に `default` エントリが**無く全 opt-in**（`ci.yml` は windows-latest・GPU なし）→ AccessKit テストは feature 追加なしで回る。`ci.yml:68-69` に `cargo test -p snotra-settings` 実在、cargo autodiscovery で `tests/settings_ui.rs` を自動収集 → **CI 変更不要**（実測前でも確定）。`docs/build-commands.md:17` に該当コマンド既存。
- CLAUDE.md 方針例外拡張 + `tests/` モジュール構成追記は計画に含む（AGENTS.md:42 準拠）。SPEC.md 更新不要は妥当。

**軽微な懸念（実装時に注意）**
1. `ci.yml:72` の clippy は `--all-targets -- -D warnings` → 新規 integration test も lint 対象。`tests/settings_ui.rs` は clippy クリーン必須。
2. `egui_kittest` はデフォルト feature ゼロのため `default-features = false` は冗長。ただし将来版が default を追加した際の防御として**明示 + コメント**で残す（`# wgpu/snapshot を引き込まない意図`）。

### セルフレビューチェックリスト（Step 5b）

1. **対称コードパス**: `ui()`/`new()` に対称ペアなし。テストが行使する Create/Edit/Delete・Discard/Save は既存の状態機械で、新規ペアを追加しない。
2. **影響範囲の網羅性**: `_frame`・`ui()`・`new()`・`list_system_fonts` の呼び出し元を grep 済み（上記）。
3. **境界条件**: モーダル境界チェック（`common.rs` の index 陳腐化ガード）は既存テスト対象。バリデーション最小値・空入力を Phase 2 シナリオに含む。
4. **リソース管理**: 新規リソース（listen/子プロセス/AtomicBool）の生成なし。純テストコード + dev-dependency のみ。
5. **既存パターン整合**: egui 描画は書き換えず `ui_impl` 委譲のみ。テスト用 DI は `common.rs` の既存パターン（純ロジック分離）と整合。
6. **YAGNI 違反**: wgpu スナップショットを**採らない**判断で範囲を絞る。fallback スクショは必須スコープ外（別 issue 候補）。
7. **シンプル化**: `new_for_test` を増やすより `list_system_fonts` を「起動時に渡す引数」に変える方が薄いかは Phase 1 スパイクで実測比較（薄い方を採る）。新状態フラグ・Mutex・子プロセスの導入なし。
8. **破壊不変条件**: Win32 フック/ホットキー/IPC に触れない。唯一の runtime リスクは `ui_impl` 切り出しのレイアウト崩れ = `cargo run -p snotra-settings` 視覚スモークで検知（不変条件に明記済み）。

# plan — issue #558: Alt 押下中の残留 Alt 解除注入をスキップ

## 方針（1 文）

`show_and_focus_main` の末尾、`send_alt_key_up()`（`src-tauri/src/main.rs:423`）の直前に「物理 Alt が押下中なら注入せず早期 return」ガードを追加する。スパイク f332a83 の `focus_and_release_alt` と同一の呼び出し側配置。

## 変更ファイル一覧

### `src-tauri/src/main.rs` — `show_and_focus_main`（388–424 行）

現状（422–423 行）:

```rust
    // Clear lingering Alt modifier after focus is confirmed.
    send_alt_key_up();
```

変更後:

```rust
    // 残留 Alt の解除は、物理 Alt が既に離れているのに、フォーカス遷移で key-up が
    // ウィンドウへ届かなかったときにしか意味を持たない。物理 Alt が押下中に注入すると
    // OS の論理修飾状態だけが解放されて物理状態と乖離し、Alt を押し直す（またはキー
    // リピートが keydown を再送する）まで Alt+Q が発火しない不感帯を作る（#558、Alt
    // 解放待ちがタイムアウトした show 経路で実測）。押下中なら解放時に自然な key-up が
    // 届くため注入は不要。SNOTRA_TRACE 有効時はスキップを可観測にし、手動再現時に
    // ガード発火を確認できるようにする。
    if is_alt_pressed() {
        trace_main("show_main:alt_release_skipped", json!({}));
        return;
    }
    // Clear lingering Alt modifier after focus is confirmed.
    send_alt_key_up();
```

- **配置**: WM_NULL 同期待ち（406–420）の後・`send_alt_key_up()` の直前。既存の順序（`show` → `main_visible=true` → `set_focus` → WM_NULL 待ち → 注入）は変えない。判定を注入決定に最も近い位置で再評価するのが要——alt-wait 経路では `wait_alt_release_or_timeout()`（最大 350ms）+ WM_NULL 待ち（最大 100ms）を経ており、押下状態が待機開始時点からドリフトしうる。
- **`#[cfg]` 不要**: `is_alt_pressed()` は非 windows で `false`、`send_alt_key_up()` は非 windows で no-op。ガードは off-Windows でそのままコンパイルされ常に no-op（現状維持）。
- **`send_alt_key_up()` 本体は変更しない**: 純粋な注入プリミティブのまま保ち、注入するか否かの判断は唯一の呼び出し元（`show_and_focus_main`）に置く（スパイクと同じ責務配置）。

### `src-tauri/src/main.rs` — `show_and_focus_main` の docコメント（380–387 行）

現状の "Order (must not change): … → Alt key-up (must follow confirmed focus transfer …)" は無条件注入を前提とした記述。ガード追加後は末尾に一言「物理 Alt が押下中なら注入をスキップ（#558）」を足し、docと実装の乖離を防ぐ（Step 2 偵察の軽微提案）。処理順序自体は変えないため "must not change" の主張は保つ。

差分規模: 1 ファイル・+約 9 行（ガード 3 行 + コメント + docコメント 1 行）/ -0。

## 実装順序

単一フェーズ。ガードを 1 箇所追加するのみで依存関係なし。

1. `show_and_focus_main` にガードを挿入。
2. `cargo clippy`（PostToolUse hook が自動発火）で沈黙＝合格を確認。
3. コミット（feature ブランチ `fix/alt-release-deadzone`）。

## 不変条件

各経路で「注入されるか」が正しく分岐すること（`research.md` の 3 経路追跡が根拠）:

- **経路 1（hide）**: `send_alt_key_up` に到達しない — 影響なし。
- **経路 2・解放 return**: 注入時 `is_alt_pressed()`=false → 注入実行（フォーカス遷移で落ちた key-up の補償を維持）。
- **経路 2・タイムアウト return**: 注入時 `is_alt_pressed()`=true → **スキップ**（バグ修正の本体。論理 Alt を物理と一致させたまま維持）。
- **経路 3（直接 show）**: 注入時 `is_alt_pressed()`=false → 注入実行（現状維持）。仮に 795 の判定後に Alt が押下されても、スキップは無害。

**スキップの安全性（1 行修正が「部分パッチ」でなく完結する根拠・Step 2b 独立導出）**: `send_alt_key_up()` には 2 つの効果がある。(1) OS のグローバル論理修飾状態（`RegisterHotKey` が Alt+Q を照合する対象）— ガードが止めるべき本体。(2) 検索ウィンドウ側の残留 Alt クリーンアップ（素の Alt-up によるメニュー起動・ビープ・次打鍵の Alt+char 化の防止）。ガードは (1) を止めるが、(2) の役割は既存の重複防御が肩代わりするためスキップしても再燃しない:

- メインウィンドウは `decorations: false`（メニューバー無し）→ 素の Alt-up は無害
- `setup_accelerator_handler` の `AcceleratorKeyPressed` が `WM_SYSKEYDOWN`（Alt+char）を `SetHandled(true)` で消費しビープを根絶
- `ui/src/components/SearchWindow.tsx:163` が `altKey && !ctrlKey && key.length===1` を `preventDefault` で握りつぶす（最後の砦）
- 物理 Alt を離した瞬間、自然な Alt-up がフォーカス済みの検索ウィンドウへ届き状態を解消

**第 2 注入点の不在（同型バグ散在なしの証拠・Step 2b）**: ツリー全体を `SendInput|GetAsyncKeyState|VK_MENU|key-up|SetForegroundWindow` で grep → Alt 注入経路は `main.rs:423` の単一チョークポイントのみ（`platform/tray.rs` の `SetForegroundWindow` は `TrackPopupMenu` 用で無関係、`snotra-settings` 別バイナリにも第 2 経路なし）。show 経路は 4 つ（ホットキー alt-wait / 直接 / second-instance / 起動時）だが**すべて `show_and_focus_main → send_alt_key_up` を通る**ため、1 箇所のガードで全経路に効く。経路別の同型パッチ散在は不要。

失敗・異常時の挙動:

- ガードは**新たな状態フラグ・プロセス・リソースを導入しない**（純関数 `is_alt_pressed()` の 1 回評価と早期 return のみ）。「戻す経路」を要する副作用が無いため、`false` 復帰・破棄のペアは発生しない。
- 早期 return は `show_and_focus_main` の**末尾**で起きる（`send_alt_key_up` が最後の文）。return 後に実行されるべき後続処理は無いため、スキップしても他の show ステップ（IME 制御・`emit_window_shown`・末尾 resume）は `show_main_and_emit` 側で正常に続行する。
- `trace_main` は `SNOTRA_TRACE` 無効時は no-op。トレース失敗がフローに影響しない。

## テスト方針

- **自動テスト追加なし（理由付き）**: `is_alt_pressed()`（`GetAsyncKeyState`）・`send_alt_key_up()`（`SendInput`）は Win32 の live 入力状態を読む/書くため、ユニットテスト不能（`src-tauri/CLAUDE.md`「Win32 依存モジュールはユニットテスト前提にしない」）。判定ロジックは 1 行のガードで、純述語として切り出す論理が無い（YAGNI）。スパイク側 f332a83 も同修正でテストを追加していない。
- **回帰の検知手段 = 手動スモーク（破壊不変条件の検知手段）**:
  1. リリース相当ビルドを起動（`docs/build-commands.md` 参照）。`SNOTRA_TRACE=1` を設定。
  2. Alt を押したまま Q を押し、**Alt を離さず 350ms 以上保持**して show をタイムアウト経路で発火させる（trace `hotkey:alt_wait_start` → `hotkey:alt_wait_done`）。
  3. show 時に trace `show_main:alt_release_skipped` が出ることを確認（＝ガード発火）。
  4. **Alt を押したまま Q を再押下** → ホットキーが即座に発火する（表示/トグル）ことを確認。修正前はここで不感帯（無反応）だった。
  5. 対照: Alt を素早く離してから show する通常経路では `show_main:alt_release_skipped` が出ず（注入実行）、既存の Alt+char ビープ抑止が維持されることを確認。
- **自動検証（沈黙＝合格）**: `*.rs` 編集で PostToolUse hook が `cargo clippy` + `snotra-tauri` のテストを自動発火。失敗時のみ会話に届く。

## SPEC.md 更新要否

**不要**。`research.md`「SPEC.md 波及の否定」の grep 証跡に基づく:

- SPEC.md に `send_alt_key_up` / Alt 解放待ち / 350ms タイムアウト / SendInput 注入の記述は皆無。これらは §8.6 状態機械（`Standby --> SearchVisible: hotkey-pressed`）の**下位の実装詳細**。
- 本修正は文書化済みの `hotkey-pressed → SearchVisible` トグル挙動を**復元**する（無条件注入が作った不感帯を除去）ものであり、SPEC 記載のフロー・IPC 契約・状態遷移を**変えない**。→ AGENTS.md Step 0 の「バグ = SPEC の意図にコードを合わせる」に該当。

## セルフレビュー（Step 5）

### 5a. `/plan-review` 結果

- **要対処**: なし（Step 2 偵察・Step 2b 独立導出とも矛盾ゼロ）。
- **軽微改善（取り込み済み）**: `show_and_focus_main` の docコメントにスキップ条件を一言追記 → 上記「変更ファイル一覧」に反映。
- **他の check スキル**: `/race-check`（新規 async fn なし）・`/symmetric-check`（生成/破棄・show/hide の対称ペア追加なし。`send_alt_key_up` は片方向クリーンアップ）・`/state-check`（UI モード・状態遷移の追加なし）・`/cache-check`・`/persistence-check`（無関係）はいずれも **N/A**。
- **Step 2b 独立導出との差分**:
  - 漏れ（導出 ∖ plan）: **なし**。独立導出が挙げた必要変更集合は本計画と同一（1 ファイル・`show_and_focus_main` のガード）。
  - スコープ過剰（plan ∖ 導出）: **なし**。
  - 一致（完全性の証拠）: 配置箇所・`#[cfg]` 不要・SPEC 不要・Win32 で自動テスト不能・チョークポイント単一性——主要判断がすべて独立に再一致。加えて独立導出が「スキップの安全性（4 重防御）」「第 2 注入点の不在」を能動的証拠として提示 → 計画へ反映済み。

### 5b. plan-review が扱わない 3 観点

1. **境界条件**:
   - タイムアウト境界（Alt を 350ms 保持 → タイムアウト経路で show）= バグ発火の本体。手動スモーク手順 2–4 で 1 件検証。
   - 795 判定（false）と注入直前ガードの間で Alt 再押下 = ドリフト。ガードが注入時点で再評価するため安全（スキップは無害）。
   - 解放境界（wait が解放で return）= 注入実行を維持。手動スモーク手順 5 の対照で検証。
   - 非 windows = コンパイル時 no-op（`is_alt_pressed()`=false → 注入 no-op）。
2. **シンプル化の挑戦**: 既に最小（ガード 1 つ・新規状態ゼロ）。より単純な「注入を丸ごと削除」は不可——経路 2 解放時・経路 3 の「フォーカス遷移で落ちた key-up の補償」を壊す。純述語の切り出しは YAGNI。この一行が最小十分。
3. **破壊不変条件 + 検知手段**:
   - 不変条件 A「Alt 押下保持中でも Alt+Q トグルが発火する」→ 検知: 手動スモーク手順 4（Alt 保持のまま Q 再押下で即発火）+ trace `show_main:alt_release_skipped`。
   - 不変条件 B「既存のビープ / メニュー / Alt+char 抑止が退行しない」→ 検知: 手動スモーク手順 5（通常経路で注入実行・ビープなし）+ 上記 4 重防御の存在（コードで裏取り済み）。

### 総評

- 計画の completeness: **高**（Step 2 全観点「問題なし」・Step 2b 完全一致・漏れ 0）。
- 実装着手可否: **可**。`/implement` へ進める。

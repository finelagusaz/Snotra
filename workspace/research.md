# research — issue #558: Alt 押下中の残留 Alt 解除注入が Alt+Q の不感帯を作る

## issue の要約

Alt+Q でウィンドウを表示した直後、Alt を押し続けたまま再度 Q を押しても、しばらく（Alt を押し直すか、キーリピートが keydown を再送するまで）ホットキーが発火しない瞬間がある。

**根本原因**: show 経路が、フォーカス移動後に残留 Alt を解除するため `send_alt_key_up()`（`SendInput` で Alt key-up を注入）を**無条件に**呼ぶ。Alt 解放待ち（`wait_alt_release_or_timeout`）が**タイムアウト**で抜けたケースでは、ユーザーは物理 Alt を押したままである。その状態で key-up を注入すると、**OS の論理修飾状態だけが「解放済み」になり物理状態と乖離する**。以後、物理 Alt が押されているのに修飾キー不成立となり、Q を押しても `RegisterHotKey` が発火しない不感帯が生じる。

**修正の要**: 注入は「物理キーは既に離れているのに、フォーカス遷移で key-up がウィンドウへ届かなかった」ときにしか意味を持たない。物理 Alt が押下中なら、解放時に自然な key-up が届くため注入は不要。注入直前に `is_alt_pressed()` を確認し、押下中はスキップする。

## 関連コード（すべて実在確認済み・`src-tauri/src/main.rs`）

| 要素 | 行 | 役割 |
|---|---|---|
| `ALT_RELEASE_POLL_MS = 10` / `ALT_RELEASE_TIMEOUT_MS = 350` | 31–32 | 解放待ちのポーリング間隔・上限 |
| `is_alt_pressed()`（windows: `GetAsyncKeyState` VK_MENU/L/R）/（非windows: `false`） | 83–97 | 物理 Alt 押下判定 |
| `wait_alt_release_or_timeout()` | 99–116 | 押下中なら最大 350ms、10ms 間隔で解放をポール。解放 or タイムアウトで return |
| `send_alt_key_up()`（windows: mask key + Alt up 注入 + 5ms sleep）/（非windows: no-op） | 124–171 | 残留 Alt を `SendInput` で解除。**単一呼び出し元** |
| `show_and_focus_main()` | 388–424 | `show()` → `main_visible=true`(395) → `set_focus()` → WM_NULL 同期待ち(406–420) → **`send_alt_key_up()`(423, 関数末尾)** |
| `show_main_and_emit()` | 456–497 | show の総合エントリ。`show_and_focus_main` を呼ぶ |
| ホットキー listener の分岐 | 782–810 | visible+toggle→hide / **`is_alt_pressed()`→spawn して `wait_alt_release_or_timeout()`後 show**(795–806) / else→直接 show(807–809) |

### 3 経路の追跡（`send_alt_key_up` に到達するのは経路 2・3）

1. **経路 1（hide）**: visible && toggle → `w.hide()`。`send_alt_key_up` に到達しない。
2. **経路 2（Alt-wait show）**: `is_alt_pressed()` が真 → 別スレッドで `wait_alt_release_or_timeout()` → 世代チェック → `show_main_and_emit`。
   - 待機が**解放**で return → 注入時 `is_alt_pressed()`=false → 注入は「フォーカス遷移で落ちた key-up」を補う（現状維持）
   - 待機が**タイムアウト**で return（＝物理 Alt 押下中）→ 注入時 `is_alt_pressed()`=true → **これが不感帯を作るバグ経路**
3. **経路 3（直接 show）**: line 795 で `is_alt_pressed()`=false → `show_main_and_emit`。注入時も押下なし → 注入実行（現状維持）。

## 既存パターン（再利用対象）

- **`is_alt_pressed()` は既に存在**（`GetAsyncKeyState` ベース、windows/非windows の両 cfg 変種あり）。新規 API は不要。ガードはこの既存関数を注入直前に再評価するだけ。
- **`trace_main(event, data)`**（`SNOTRA_TRACE` ゲート）が `show_and_focus_main` 内で多数使われている（`show_main:show:start` 等）。スキップ経路に 1 行 trace を足せば、手動再現時に「ガードが発火した」ことを観測できる（＝修正の検知手段）。
- **スパイク側（#532）に完全な前例あり**: `snotra-egui-mvp/src/soft_host_main.rs` / `glow_park_host_main.rs` の `focus_and_release_alt` で、WM_NULL 待ちの直後・`send_alt_key_up()` の直前に `if is_alt_pressed() { …return; }` を配置（呼び出し側配置・早期 return）。製品版の `show_and_focus_main` と同一構造。

## 技術的制約

- **`GetAsyncKeyState` / `SendInput` は Win32 の live 状態を読む/書く**ため、ユニットテスト不能。純述語ラッパーの抽出は YAGNI（判定ロジックは 1 行のガードで、切り出す論理が無い）。→ 検証は手動再現に依る。
- **`SendInput` はシステム入力キューに注入し、ルーティングはキュー取り出し時に決定**（`src-tauri/CLAUDE.md`）。フォーカス移行直後の注入が対象へ届かない場合があるため、既存コードは `SendMessageTimeoutW(WM_NULL)` でフォーカス完了を同期待ちしてから注入している。**本修正はこの順序に手を入れない**——ガードは WM_NULL 待ちの後・注入の直前に挿入するのみ。
- **物理状態と論理状態の乖離**が本バグの本質。ガードは冪等性ではなく「注入の**必要性**」を判定する（物理押下中は将来 key-up が自然に届くので不要）。
- 非 windows では `is_alt_pressed()`=false・`send_alt_key_up()`=no-op のため、ガードは `#[cfg]` 不要でそのままコンパイル・動作（off-Windows は常に no-op）。

## 外部ソース（#532 スパイク）の確認結果

- スパイク修正コミット **f332a83** は `origin/feat/532-egui-mvp` に現存し、**revert・supersede されていない**（`git log origin/feat/532-egui-mvp -- soft_host_main.rs` で確認。後続 c887fbd は fix 1 の完了で別件、ガード行 1006–1007 は健在）。
- f332a83 は **2 つの独立修正**を含む:
  1. **visible フラグの楽観 true**（スパイク固有）— コミット本文が「製品版と同じく真実源」と明記。製品版は `main_visible.store(true)` を実際の show 時（main.rs:395、`show()` 後）に行い、Alt-wait 経路は spawn 前に visible を立てない。**製品版に fix 1 の問題は無い（移植不要）**。
  2. **Alt key-up 注入ガード** — これが #558 で移植すべき唯一の修正。
- スパイク側の観測ログ手段は `eprintln!`（デバッグハーネス用）。製品版は `SNOTRA_TRACE` ゲートの `trace_main` を使うため、移植時はそちらへ読み替える。

## 未解決の疑問

なし。issue の前提はコードで裏取り済み、外部ソースは最終コミット状態まで確認済み、SPEC.md への波及も grep で否定済み（`workspace/plan.md` 参照）。

## SPEC.md 波及の否定（grep 証跡）

- `send_alt_key_up` / `wait_alt` / 残留 / SendInput / 350 / タイムアウト / release_or_timeout / 押下中 を SPEC.md で grep → **Alt 解放待ち・注入タイミングの記述は皆無**（ヒットは kana_query の「残留」・起動 OS 呼び出しの「タイムアウト」で無関係）。
- SPEC §8.6 状態機械は `Standby --> SearchVisible: hotkey-pressed` を規定するが、Alt-release-wait 機構はその**下位**の実装詳細。本修正はこの状態遷移を**復元**する（無条件注入が壊していたトグルを直す）ものであり、遷移そのものは変えない。
- → **SPEC.md 更新不要**（実装詳細のバグ修正）。

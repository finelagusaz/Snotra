# research: #660 `snotra-egui-mvp` を除去する

## issue の要約

Issue #532 の採用判断（Phase 1 技術スパイク）用に作った検証バイナリ crate `snotra-egui-mvp` を、リポジトリから削除する。issue が付けたタイミング条件は「SU7 が落ち着いた時」。

**タイミング条件の現況（前提として明記する）**: SU7 の実装 3 部作（PR1 smoke #659 / PR2 flip #661 / PR3 フロント撤去 #662）はマージ済みで、製品既定は egui。残るのは実機 GUI smoke と配布リリース手順（#532 close 待ち）。SU7 配布・検証 spec（`docs/superpowers/specs/2026-07-24-su7-distribution-verification-design.md`）を `mvp` で grep して 0 件——**残作業は本 crate に依存しない**。着手は `/start-issue 660` を起動したユーザーの判断とし、確認は求めない。

**「検証記録として残る」との整合**: `snotra-egui-mvp/README.md` は「本 crate は Phase 1 の検証記録として残る」と書く。削除後も記録は (a) git 履歴、(b) `docs/superpowers/specs|plans` の SU1〜SU7 系 8 本以上（`governanceDocs()` が除外する歴史資料・#589 で非規範化）に残る。**ゆえに `docs/superpowers/**` は書き換えない**——歴史資料の改変は記録の破壊であり、参照実在検査の対象外でもある。

## 参照の全数列挙（SSOT = ripgrep への 1 回の無制限問い合わせ）

`snotra[-_]egui[-_]mvp|egui[ -]?mvp|SNOTRA_EGUI_MVP` を glob 制限なしで走らせた結果を母集団とする（クレート名だけの grep では `docs/architecture.md:46` の散文「Issue #532 egui MVP（非配布）」のような行が漏れる。glob 付き grep が偽陰性を出した実例あり——AGENTS.md「列挙は SSOT のツール自身に問う」）。

### A. 削除対象（crate 本体・追跡 7 ファイル）

`snotra-egui-mvp/`: `.gitignore` / `CLAUDE.md`(20) / `Cargo.toml`(28) / `README.md`(28) / `build.rs`(3) / `src/main.rs`(770) / `tauri.conf.json`(26)

### B. ワークスペース定義

| 参照元 | 内容 |
|---|---|
| `Cargo.toml:4` | `members` の `"snotra-egui-mvp"` |
| `Cargo.lock:4786` | `[[package]] name = "snotra-egui-mvp"`（`cargo check` が再生成） |

### C. 製品コード（mvp のためだけに残していたもの・要変更）

| 参照元 | 内容 | 根拠 |
|---|---|---|
| `snotra-egui-runtime/src/runtime.rs:34` | `RuntimeFrame::close_window()` + `close_requested` フィールド(:29) + `apply_frame_commands` の close 分岐(:391-393) | **呼び出し元は `snotra-egui-mvp/src/main.rs:185` の 1 件のみ**（全 `*.rs` grep で実測）。`docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md:162` が「`close_window` は `snotra-egui-mvp` のみだが **#660 の削除まで温存する**」と明記——削除の予定地は当 issue |
| `snotra-egui-runtime/src/runtime.rs:93` | `WindowWaker` に `#[must_use]` を付けない理由として mvp を名指す doc コメント | 理由節のみが mvp 依存。方針（`attach` の戻り値は捨ててよい）は独立 |
| `snotra-egui-runtime/src/raster.rs:3` | `spike snotra-egui-mvp/src/soft_host_main.rs から移植` | **既に stale**（`soft_host_main.rs` は SU1 で削除済み・現存しない） |
| `snotra-core/tests/search_frame_cost.rs:10` | 合成インデックス生成規則の由来として `snotra-egui-mvp` の `build_verification_engine` を参照 | 削除後は宛先のないパス参照になる |

### D. セーフティネット（hook / CI）

| 参照元 | 内容 |
|---|---|
| `.claude/hooks/post-edit.mjs:38` | 出力予算 `"egui-mvp-test"` |
| `.claude/hooks/post-edit.mjs:115` | `selectChecks` の `snotra-egui-mvp/` 分岐 |
| `.claude/hooks/post-edit.mjs:275-276` | `case "egui-mvp-test"` → `cargo test -p snotra-egui-mvp` |
| `.claude/hooks/post-edit.test.mjs:104-112` | `selectChecks("snotra-egui-mvp/src/main.rs")` の期待値 |
| `.claude/hooks/post-edit.test.mjs:602` | ルート `Cargo.toml` members カナリアの crate 列 |
| `.claude/hooks/post-edit.test.mjs:613,617` | 予算エントリ検算の `REPRESENTATIVE_EDITS` とその由来コメント |
| `.github/workflows/ci.yml:85-86` | `cargo test (snotra-egui-mvp)` ステップ |

### E. ガバナンス文書（`governance:check` の対象）

| 参照元 | 内容 |
|---|---|
| `AGENTS.md:10` | 第2層の実装事実に `snotra-egui-mvp/src/*.rs` |
| `AGENTS.md:20` | モジュール固有不変条件の CLAUDE.md 列挙 |
| `docs/architecture.md:11` | ディレクトリ図の見出しコメント「workspace（製品3 crate + egui検証2 crate）」＝**件数の写し** |
| `docs/architecture.md:13` | `egui/wgpu 接着層`＝削除行の隣にある同型の嘘（SU1 以降 softbuffer） |
| `docs/architecture.md:14` | ディレクトリ図の `snotra-egui-mvp/` 行 |
| `docs/architecture.md:20` | 同図直後の散文（長行・要確認して該当句のみ削る） |
| `docs/architecture.md:46-47` | レイヤー図の「Issue #532 egui MVP（非配布）」ボックス |
| `docs/architecture.md:76,78,80` | 見出し「snotra-egui-runtime / snotra-egui-mvp（Issue #532 検証層）」+ 散文 + `snotra-egui-mvp/CLAUDE.md` への参照 |
| `docs/build-commands.md:18` | カテゴリ A の `cargo test -p snotra-egui-mvp` |
| `docs/build-commands.md:24` | 「全 5 crate」＝**件数の写し** |
| `docs/build-commands.md:25` | PostToolUse 発火パス列挙（長行）に `snotra-egui-mvp/**` |
| `docs/build-commands.md:26` | 「6 つ目の crate を追加したとき」＝**件数の写し**（plan-review の独立再導出が拾った） |
| `docs/build-commands.md:52` | カテゴリ D の「mvp を起動しても製品の変更は映らない」注記 |
| `docs/build-commands.md:87` | その他コマンド一覧のテスト行 |
| `docs/build-commands.md:95` | `cargo run -p snotra-egui-mvp` 行 |
| `docs/build-commands.md:128` | CI/CD メモ対応表の rust-check 行 |
| `docs/development-principles.md:13` | 責務集約先の CLAUDE.md 列挙に `snotra-egui-mvp/CLAUDE.md` |
| `.claude/skills/retrospective/SKILL.md:65` | 教訓の振り分け先 CLAUDE.md 列挙 |

**件数の写しは 3 件**（`docs/architecture.md:11`・`docs/build-commands.md:24`・`:26`）。crate 名を含まないため crate 名 grep では到達せず、**どの機構も検知しない**——crate を 1 つ減らした瞬間に嘘になる（`[0-9]+\s?crate` の全 `*.md` grep で列挙。plan-review の 3 体が独立に同じ 3 件へ到達）。

### F. 触らない（歴史資料）

`docs/superpowers/plans|specs` の 10 ファイル・30 行以上（SU1/SU2/SU4/SU5/SU6/SU6.5 の plan・spec、#671 サイクルの PR A / PR D plan、phase2 ロードマップ）。`governanceDocs()` が `docs/superpowers/` を除外するため参照実在検査も掛からない。

## `snotra-egui-mvp/CLAUDE.md` 不変条件 9 件の帰着（削除で失う知識の棚卸し）

ファイルが消えると取り返せないのはここだけなので、1 行ずつ判定する。

| # | 不変条件 | 帰着 |
|---|---|---|
| 1 | 製品版 `src-tauri` の既定起動経路・設定を変更しない | **crate と共に消える**（mvp が製品を汚さないための制約。対象が消えれば命題も消える） |
| 2 | `app.windows` は空・Window は `tauri::Window::builder` だけで生成 | **製品の事実として既に存在**: `src-tauri/tauri.conf.json:9-11` が `"app": { "windows": [] }`（設定ファイル自身が SSOT・SU7 でフロント撤去）＋ `src-tauri/CLAUDE.md:70`「ウィンドウ生成の制約」が setup 限定を規範化 |
| 3 | Updater 確認中に egui イベントループをブロックしない | **製品に移設済み**: `src-tauri/src/egui_shell/mod.rs:119-177`（SU5・`spawn_update_check` が非同期・SPEC §20.2） |
| 4 | 検証用履歴はプロセス固有の未使用一時パスから読むだけ | **crate と共に消える**（`main.rs:356` の検証専用パス） |
| 5 | Alt+Q 表示は Alt 解放待ち → フォーカス確認 → 残留 Alt 解除の順序 | **製品が本体**: `egui_shell/lifecycle.rs:47`（`HotkeyPlan::ShowAfterAltRelease`）・`egui_shell/mod.rs:401`（残留 Alt 解除・#558）。mvp 側が製品を写していた |
| 6 | hide/show 反復・日本語 IME 強制は `SNOTRA_EGUI_MVP_*` env でのみ有効化 | **crate と共に消える** |
| 7 | Windows フォントの static 保持は `OnceLock`（再表示ごとのリークを作らない） | **製品に移設済み**: `egui_shell/view.rs:120` `jp_font_bytes()` が `OnceLock`。動機の実測は `PERFORMANCE.md:176`「user_font も `from_static` で積む」 |
| 8 | **warm frame の日次比較はしない**（同一ホストで日により 3 倍変動・2026-07-17 実測。比較は必ず同日・同条件） | **孤立**——全 `*.md` を grep して当該行以外に 0 件。計測方法論であり crate に依存しない → `PERFORMANCE.md`「計測と受け入れ基準」(:239) へ移設する |
| 9 | 混在スクリプトは `jp_font` を families の先頭へ（`insert(0, …)`）。理由: softbuffer のカバレッジ AA 欠如が vertical metrics の分数差を整数 px へ丸める。#399 の再発で、**新規 bin ごとに `push` を再導入していた**（#579）。型検査・clippy・単体テストを素通りし視覚スモークでのみ顕在化 | **規則は機構化済み・因果が孤立**。規則: `src-tauri/src/egui_shell/view.rs` の 4 テスト（`font_definitions_fallback_is_jp_single_stack`（#579 を名指しで固定）/ `..._honor_puts_user_first_jp_fallback` / `..._covered_user_font_omits_jp_entirely` / `..._registers_both_fonts_as_borrowed`）が固定（**関数名で参照する**——当初 `:1651-1694` と書いたが実際は `:1697-1756` で、行番号は既にドリフトしていた）。周辺記述: `SPEC.md:519`（非 MS フォントのベースラインずれは視覚スモークのみで顕在化する受容残余）・`snotra-settings/CLAUDE.md:71`（#399 の混在ベースラインずれ）。**無い**のは「softbuffer ラスタが丸めで顕在化させる」因果と「bin を作るたび再導入した」再発史 → `src-tauri/CLAUDE.md` へ移設する（規則だけ残して理由を捨てると、once 再発した型のミスが再発する） |

## 既存パターン

- **crate/資産の撤去の先例**: #532 SU7 PR3（15933af）でフロント・IPC・WebView2 を撤去。`ui/CLAUDE.md` ごと削除し、`scripts/governance-check.mjs:78-83` の `G1_CRATES` から `ui` を落とすまでを 1 PR で完結させている（コメント「ui は #532 SU7 のフロント撤去で消滅」が残る）
- **`G1_CRATES` は既に `snotra-core` / `src-tauri` / `snotra-settings` の 3 crate のみ**——`snotra-egui-mvp` も `snotra-egui-runtime` も入っていない。**G1 の編集は不要**（`governanceDocs()` も両 CLAUDE.md を母集団に含めない）
- **削除の検出器はコンパイラと governance:check**: 「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」（AGENTS.md 条件別チェック）の裏返しで、`members` の stale エントリは `cargo` の hard error、参照残りは G3/G5/G6/G9 が指す

## 技術的制約

- **`cargo check --workspace` は `members` の不在ディレクトリを hard error にする**（自己検知。削除とマニフェスト更新は同一コミットで揃える必要がある）
- **`Cargo.lock` は追跡ファイル**（`.gitignore` に無い）。`cargo check` が更新した lock を同じコミットに含める
- **`cargo doc --workspace --no-deps --document-private-items`**: root `[workspace.lints.rustdoc] broken_intra_doc_links = "deny"`。**削除する crate へ解決していた intra-doc link があれば deny で落ちる**（PostToolUse hook は `cargo doc` を発火しないため沈黙は合格でない・#562）
- **`governance:check` の掛かり方（機械的な閉じ役）**:
  - **G3**（参照実在）: `docs/development-principles.md:13` と `docs/architecture.md:80` は `snotra-egui-mvp/CLAUDE.md`＝`.md` 拡張子付きパス参照ゆえ `REF_EXTENSIONS` に合致し、消し忘れれば **red**。`snotra-egui-mvp/`（拡張子なし）や `snotra-egui-mvp/src/*.rs`（glob）は述語の対象外＝**検知されない**ので手で消す
  - **G5**: `docs/build-commands.md` の `cargo test -p snotra-egui-mvp` は crate 実在検査に落ちて **red**
  - **G6**: CI/CD 対応表 ↔ `ci.yml` の照合
  - **G9**: `post-edit.mjs` の `cargoSpec` ↔ build-commands カテゴリ A の照合。**hook と build-commands は同時に直す**
- **`post-edit.test.mjs` は hook と同一ステップで直す**——`selectChecks` を変えてテスト期待値が旧のままの中間状態を残すと、`.claude/hooks/**` 編集で自動発火する hook-selftest がその場で red になる（`REPRESENTATIVE_EDITS`(:617) と members カナリアの crate 列(:602) も同じ理由）
- **Smoke workflow は自動起動する**: 本 PR は root `Cargo.toml` と `Cargo.lock` に触るため `e2e.yml` の paths（`'**/Cargo.toml'` / `'Cargo.lock'`）に合致し、`smoke-egui` job（`smoke:startup` + `smoke:egui`）が PR で走る
- **カテゴリ D（手動 GUI 視覚スモーク）の発火条件**: 「UI のスタイル・レイアウト・テキスト表示に影響する変更」。製品の描画コードは 1 行も変えない（`close_window` は呼び出し元ゼロの死んだ分岐）ため**非該当**と判断する。文書側のカテゴリ D 注記（:52）の編集は挙動に影響しない
- Win32 API の同期性は本 issue では論点にならない（新規 API 呼び出しなし）

## 未解決の疑問（判断として plan に落とすもの）

1. **`WindowWaker` に `#[must_use]` を付けるか**——付けない。`runtime.rs:91-92` の方針文「戻り値を捨ててよい（wake が不要な窓では単に落とす）」は mvp と独立に成立しており、mvp を名指す**理由節だけ**を落とす。`docs/superpowers/plans/2026-07-25-pr-d-ctx-consolidation.md:28` は「#660 で当 crate を削除するまで壊さない」と書くが、これは付与の予告ではなく現状維持の要請。属性の追加は API 契約の変更であり issue の要求外（必要なら別 issue）
2. **`snotra-egui-runtime` の `pub` 露出のうち src-tauri から未消費のもの**（`key_from_tao` / `modifiers_from_tao` / `is_renderable_extent`）——**触らない**。crate 内部（`input.rs` / `renderer.rs`）と自 crate テストが消費しており死にコードではなく、`pub` を絞るのは issue の要求外（YAGNI 方向の整理として別途）

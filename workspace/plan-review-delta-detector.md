# plan-review（差分のみ）— #1133 アイコン抽出キーを行ごとに定める（案 C）

対象: `workspace/plan.md` の 2026-08-18 ユーザー注釈による差分のみ（Phase 0a の検知器・Phase 0b/Phase 5 の実機確認）。前回レビュー（`plan-review-instant-icon-key.md`）で指摘済みの論点は再掲しない。

## 観点 C — 検知器（`icon_gate_keeps_input_idle_semantics`）は本当に効くか

### 確認できた（問題なし）

1. **assert の綴りは今のソースに逐語で存在する。**
   - `view.rs:1159`: `let input_idle = !self.controller.is_search_armed();`（実測・完全一致）
   - `results_view.rs:660`: `if snapshot.input_idle {`（実測・完全一致）
   - 計画の「現状のコードで緑」という前提は成立する。
2. **変異 6・7 が assert を赤にすることの根拠は妥当。**
   - 変異 6（`&& self.controller.instant_rows_query().is_none()` を足す）は `view.rs:1159` の行を書き換えるため、`let input_idle = !self.controller.is_search_armed();` という逐語の部分文字列が消える（rustfmt が長い行を折り返しても、`is_search_armed()` の直後に `;` が来ない時点で不一致になるので折り返し方に関わらず赤になる）。
   - 変異 7（`if snapshot.input_idle` を外す）は `results_view.rs:660` の当該行が消えるので自明に赤になる。
3. **`Config::validate()` は本体の適用経路を通らない**（`src-tauri/src/egui_shell/layout.rs:369` のコメントで明記）——今回の検知器そのものとは無関係だが、Phase 0a のテスト設計が「述語のテストでは呼び出し点の脱落を捕まえられない」という同じ理由に立つ先例（`launcher_controller.rs:1930-1978` の `activation_entry_points_consult_the_display_gate`）と粒度が一致していることを実物で確認した。
4. **`include_str!` のクロスファイル使用は技術的には問題ない。** `results_view.rs` と `view.rs` は同じディレクトリ（`src-tauri/src/egui_shell/`）にあり、`include_str!` のパス解決はマクロを書いたファイルのディレクトリ基準なので正しく解決する。`rustc`/`cargo` は `include_str!` で読んだファイルを depinfo に含めるため、`view.rs` の編集は `results_view.rs` を含む再ビルドを正しく誘発する（cargo の標準機構であり Snotra 固有の懸念ではない）。

### ⚠ 軽微（プランには無い残る死角）

1. **既存の `include_str!` 先例（`launcher_controller.rs:1879,1958,2028` / `indexing.rs:317` / `view.rs:1408`）はすべて「自ファイルを読む」形である。** 今回計画している「`results_view.rs` の `mod tests` が `view.rs` を読む」形は本リポジトリで初めてのクロスファイル `include_str!` になる。ビルド上の実害は無いと確認済みだが（上記）、**先例と違う点として計画に一言あってもよい**——将来 `view.rs` を分割するリファクタが来たとき、この検知器が `view.rs` という特定のファイル名に固定されていることを見落とすリスクがわずかにある。
2. **計画の「残る死角」に書かれていない、もう一段具体的な迂回経路がある。** 計画は「ゲートを別ヘルパーへ移して本体に綴りが残る形」を死角として挙げているが、**より狙いやすい迂回はここに既にある `is_search_armed()` を書き換えることである**。
   - `view.rs:1159` の呼び出し元 `is_search_armed()` は `launcher_controller.rs:193-195` で 1 箇所だけ定義されている（`pub(super) fn is_search_armed(&self) -> bool { self.search_debounce.is_armed() }`）。
   - `input_idle = !is_search_armed()` なので、instant 中を idle でなくす（＝#1074 が禁じた修正と**同じ効果**を持つ）には De Morgan で `is_search_armed()` 側へ `|| self.instant_rows_query().is_some()` を足せばよい。
   - この変更は **`view.rs:1159` の 1 文字も変えない**——`let input_idle = !self.controller.is_search_armed();` という assert 対象の逐語はそのまま残る。ゆえに `icon_gate_keeps_input_idle_semantics` は**緑のまま**、#1074 が名指しで禁じた退行が入る。
   - これは計画が挙げる「別ヘルパーへ移す」死角の**具体化**というより、**既に factored-out 済みの既存ヘルパーを書き換えるだけで済む**、より少ない差分で踏める迂回である。計画の「残る死角」の記述にこの一文を足すことを推奨する（実装上の対処は不要——検知器の限界を doc に明記するだけで #1077/#1112 系の先例と同じ扱いになる）。

## 観点 D — 実機確認の手順に、実行時に判断が要る穴が無いか

### 確認できた（問題なし）

1. **既定 `instant_command_prefix` は `"@"`** （`snotra-core/src/config.rs:217`）。
2. **`"@"` を 1 打鍵した後の意味論は計画の記述と一致する。** `search_state.rs:40-61` の `interpret` は `raw_query="@"`, `prefix="@"` のとき `is_instant_prefix` が真になり、`input = ""`（空文字列）なので `QueryIntent::Instant { filter_name: "", instant_query: "" }` を返す。`filter_instant_commands`（`instant.rs:369-381`）は `input.is_empty()` のとき全件を返すので、「プレフィックスだけの入力は全件表示」という計画の主張は実装と一致する。
3. **A 側で番兵が `icon:extract_failed` に現れるという予測は成り立つ。** 現行（未実装）の `matching_results`（`instant.rs:347-361`）は `description` が非空なら `SearchResult.path = description.clone()` とする。ゆえに url 型プローブの `description` に置いた番兵文字列は、そのまま抽出キー（`path`）になる。`commands/icon.rs:80-87` の `icon:extract_failed` trace は失敗した `path` をそのまま payload に積むので、番兵文字列が trace の `"path"` フィールドに逐語で現れる——識別の仕組みは妥当。
4. **`Config::validate()`（重複名・未知の modifier 等のチェック）は本体の起動経路を通らない**（`layout.rs:369` で確認済み）。プローブ config が `validate()` の基準を満たすかは気にしなくてよい——`toml::from_str::<Config>` が parse できさえすれば起動時に使われる。

### 要対処

1. **プローブ config の Windows パスの TOML エスケープが計画に書かれておらず、書いた通りに書くと parse が落ちる。** 計画の表（「プローブ用 config」節）は `exe = C:\Windows\System32\notepad.exe` と書いているが、これをそのまま `exe = "C:\Windows\System32\notepad.exe"`（二重引用符の basic string）として書くと **TOML の parse エラーになる**——`\W` や `\S` は TOML の有効なエスケープシーケンスではない（有効なのは `\b \t \n \f \r \" \\ \uXXXX \UXXXXXXXX` のみ）。
   - このリポジトリ自身がこの罠を認識している証拠がある: `snotra-core/src/config.rs:1986` のテスト fixture は Windows パスを `path = "C:\\Tools"`（バックスラッシュを `\\` で明示的にエスケープ）と書いている。`scripts/smoke-egui.ps1:102`（`$scanDirToml = $scanDir -replace '\\', '/'`）は同じ問題を**フォワードスラッシュへの変換で回避**している。
   - **プローブ config が parse に失敗すると**、`Config::load` は `InvalidData` 枝（内容破損）に入り `config.toml.bak` へ退避した上で**既定値**（`instant_commands` は `g`/`gh` の 2 件・URL 型のみ）で起動する。これは exec 型プローブ 2 件が**存在しないまま**実機確認全体が進行することを意味し、Phase 0b（A 側）・Phase 5（B 側）とも**プローブではなく既定値**を見て「合格」してしまう——しかも url 型の `g`/`gh` は番兵文字列を含まないため、A 側の `icon:extract_failed` 検査は「観測できない」ではなく「別の理由で 0 件」という紛らわしい失敗の仕方をする。
   - **対処**: プローブ config の `exe` はバックスラッシュを `\\` にエスケープするか、フォワードスラッシュ（`C:/Windows/System32/notepad.exe`。Win32 API はどちらも受理する）にするか、TOML の literal string（単一引用符 `'C:\Windows\System32\notepad.exe'`）を使う。**加えて**、`scripts/smoke-egui.ps1:309-327` と同型の「`[config] ` 診断行が出ていないこと（parse 成功の肯定的確認）」チェックをプローブ手順にも入れることを推奨する——計画はこの確認を明示していない。

2. **Phase 5（B 側）の「陰性」判定が `SNOTRA_TRACE` の有効化を暗黙の前提にしており、計画の文面はそれを明示していない。** 「実機確認」節は隔離（`SNOTRA_CONFIG_DIR`）についてのみ `scripts/smoke-egui.ps1` の形を借りると書いており（`:47, :685` を引用——いずれも config dir 隔離・`*.bin` 生成確認の行であって `-Trace` / `SNOTRA_TRACE` の行ではない）、trace 有効化については触れていない。`trace.rs` の `icon:extract_failed` を含む全 trace 出力は `SNOTRA_TRACE` 環境変数が真のときだけ出る（既定は無効）。
   - A 側（陽性を期待）では、`SNOTRA_TRACE` を有効にし忘れると trace が**全く 0 行**になるはずなので、番兵が無いことに加えて他の trace（`hotkey:registered` 等)も無く、実装者が「有効化を忘れた」と気づける可能性は高い。
   - しかし **B 側（陰性＝番兵が現れないことを期待）では、`SNOTRA_TRACE` の有効化忘れは「番兵が出ない」という期待どおりの結果と区別が付かない**——trace 自体が 0 行でも「陰性」の条件（sentinel が無い）は満たしてしまう。計画はこの経路を「陰性の有効性は同一プロセス内の対照が担保する（exec 行にアイコンが描かれることで抽出経路が生きていると分かる）」としているが、**その対照は窓キャプチャ（画像）による確認であり、trace が出ているかどうかとは独立の経路である**ため、trace 無効化に気づかないまま「陰性は trace で確認、生存は窓キャプチャで確認」と両方を別々に「合格」と記録しうる。
   - **対処**: 手順に「trace 有効化の肯定的確認」（例: `hotkey:registered` や `egui_show:done` など、A/B 側どちらでも出るはずの trace が実際に出ていることを先に確認する）を 1 行足すことを推奨する。`Start-SnotraProcess -Trace`（`scripts/lib/SnotraSmoke.psm1:332,351`）を流用するならこの懸念は機構的に消えるが、計画にはその明記が無い。

### ⚠ 軽微

1. **`@` を打鍵する VK の決定方法が計画にない。** `scripts/smoke-egui.ps1` の `Get-LetterVk`（128-135 行）は A-Z のみを対象にしており、`@` を送る既存ヘルパーは無い。US 配列なら `Shift+2`（`Send-SnotraKeyChord -VirtualKeys @(0x10, 0x32)`）で送れるが、これは同スクリプトが `:` を `Shift+;`（`0x10, 0xBA`）で送っている既存パターン（397 行）と同型であり、目新しい問題ではない。プローブスクリプトを書く時点で 1 行足すだけで済むので致命ではないが、計画の「手順」には書かれていない。

## 分類まとめ

- **要対処**: 2 件（プローブ config の TOML バックスラッシュエスケープ／`SNOTRA_TRACE` 有効化の明示的確認欠如）
- **軽微**: 3 件（`is_search_armed()` 書き換えによる検知器迂回が「残る死角」に未記載／`include_str!` のクロスファイル使用が本リポジトリ初／`@` 打鍵の VK 決定方法が手順に未記載）
- **未検証**: なし（本レビューはコード読み取りのみで完結し、実機・ビルドは実行していない。今回の 2 つの観点についてはソースコードの実測で裏取りできたため、レビュー方式上の「未検証」項目は無い）

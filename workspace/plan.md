# plan — #1133 アイコン抽出キーを行ごとに定める（案 C）

**種別: 仕様変更**（`SPEC.md` に記述があり、それを**変える**）。順序は `SPEC.md` → コード → ドキュメント。

## 目的

SPEC §19.5 の宣言（instant モード中はアイコン取得をスキップ）と egui 経路の as-built の乖離を、**仕様を変える**方向で解消する。結果行のアイコン抽出キーを**行ごとに定まる値**として仕様・実装・検知器の 3 つで揃え、instant 行では exec 種別は exe のアイコンを出し、URL / Legacy 種別は抽出そのものを行わない。

## 受け入れ条件

1. `SPEC.md` §3.4 に「アイコン抽出のキーは行ごとに定まる」規則が**正本として 1 か所だけ**あり、§19.5 は instant 固有の写像（種別 → キー）だけを書いてそこを参照する
2. URL 種別・Legacy 種別の instant 行に対して**抽出要求が 1 件も積まれない**（純関数の単体テストで決定的に測る）
3. exec 種別の instant 行は、`args` の有無・`description` の有無に依らず `expand_env(exe)` をキーとして抽出される
4. 平文検索・フォルダ・tool・履歴・トレイの行の挙動は**不変**（キーは `path` のまま・追加確保なし）
5. 抽出キーを読む箇所（要求・テクスチャ引き・可視集合での剪定）が**単一の導出**を使い、**どれか 1 か所を `path` へ戻す変異でテストが落ちる**
6. `cargo test` / `cargo clippy -D warnings` / `npm run governance:check` が green

## 変更ファイル一覧と対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-core/src/ui_types.rs` | `IconSource`（新）/ `SearchResult.icon`（新フィールド）/ `SearchResult::icon_key`（新メソッド） | 型の導入と単一導出 |
| `snotra-core/src/instant.rs` | `matching_results` / `icon_source_of`（新・private） | 種別 → キーの決定。`env_expand` 引数の追加 |
| `snotra-core/src/folder.rs`, `snotra-core/src/instant.rs`, `snotra-core/src/search.rs`, `snotra-core/src/search/scoring.rs` | `SearchResult { .. }` 構築点 | `icon` を足す。**一覧は目安であり正本ではない**——尽きたことを決めるのはコンパイラである（`-D warnings` 下の E0063） |
| `src-tauri/src/egui_shell/icon_textures.rs` | `wanted_icon_keys`（新）/ `visible_icon_keys`（新）/ `icon_for_row`（新）/ `IconMsg` の doc（:8） | 3 つの読みを純関数へ切り出す（テスト可能にする）＋「path キー」の語を直す |
| `src-tauri/src/egui_shell/results_view.rs` | `request_icons_for_results` / `update` の剪定 / `results_list_ui` / doc・コメント（:198, :570） | 上の純関数へ寄せる＋「path キー」の語を直す |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `matching_results` 呼び出し（:928 付近） | `expand_env` を渡す |
| `src-tauri/src/egui_shell/{search_state,results_view}.rs`, `src-tauri/src/platform/tray.rs` | `SearchResult { .. }` 構築点 | 同上 |
| `SPEC.md` | §3.4 / §19.5 | 規則の正本を §3.4 へ、§19.5 は写像＋参照 |
| `docs/architecture.md` | 「アイコンパイプライン」の `path キーで stale 無害` の行（:112） | 「行ごとのキー」へ更新（**この変更で偽になる記述**） |
| `docs/adr/ADR-instant-row-icon-key.md`（新） | — | 案 A・案 B の却下理由（否定の知識・U4） |

### 型（案）

```rust
/// 結果行のアイコン抽出キーの出所（`SPEC.md`「3.4 アイコン」）。
///
/// **derive は `SearchResult` と同じ集合を持つこと**——あちらが `PartialEq, Eq`
/// （`RowsSnapshot::matches` の行比較が依存）と `Serialize, Deserialize`（消費者は
/// 無いが derive は生きている・#836）を derive しているため、欠けると壊れる。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum IconSource {
    /// `path` をそのままキーにする（ファイルを指す行の既定）。
    #[default]
    FromPath,
    /// 抽出しない（`path` がファイルを指さない行）。
    Skip,
    /// 別のファイルをキーにする（instant の exec 種別の exe）。
    Explicit(String),
}
```

`FromPath` を既定にすることで、平文検索の 200〜1000 行に**追加の `String` 確保が乗らない**（`Some(path.clone())` 方式を採らない理由・research.md 技術的制約 4）。

## 実装順序

**Phase 0 は実装より前に置く**（ユーザー注釈・2026-08-18）。検知器が現状で緑になること・変異で赤になることを、**実装差分がまだ 1 行も入っていない場所で**測る。A 側の実機ベースラインも同じ理由でここに置く（後から取り直すには main のビルドへ戻る手間が乗る）。

- **Phase 0a（検知器を先に置く）**: `input_idle` の意味論とアイコン抽出ゲートの位置を固定するソーステキスト検査を追加し、**現状のコードで緑**であることと、**変異 6・7 で赤**になることを実測する（この時点で 1 コミット）
- **Phase 0b（実機ベースライン A 側）**: 下記「実機確認」の A 側を採る

**Phase 1〜3 は 1 コミットにする**——`SearchResult` にフィールドを足した瞬間から全構築点が compile-fail になり、間の状態はビルドが通らない（AGENTS.md「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」）。

- **Phase 1（snotra-core）**: `IconSource` / `SearchResult.icon` / `icon_key()` を入れ、全構築点を `FromPath` で移行。`matching_results` に `env_expand` を足し、種別 → キーを決める。単体テストを追加
- **Phase 2（src-tauri 消費側）**: `wanted_icon_keys` / `visible_icon_keys` / `icon_for_row` を `icon_textures.rs` に新設し、`results_view.rs` の 3 か所をそこへ寄せる。単体テストを追加
- **Phase 3（配管）**: `launcher_controller` が `crate::commands::launch::expand_env` を `matching_results` へ渡す
- **Phase 4（仕様・文書）**: `SPEC.md` §3.4 / §19.5、`docs/architecture.md` を更新（別コミット）
- **Phase 5（検証）**: 変異注入と全カテゴリの検査

## 不変条件と異常系

- **`input_idle`（`RowsSnapshot`）の意味論を変えない。** #1074 で固定されており、doc が「perf ヒューリスティックであって正しさの述語ではない」「`is_unsettled` と同じ修正を当ててはならない」と明記している。案 C は**キー側**でスキップを表現するので、このゲートに触れずに無駄仕事が消える
- **`icons.bin` の形式は変えない。** 変わるのは**キーの中身**だけ（instant exec 行が display 文字列 → 実 exe パス）。version バンプは不要。旧キーのエントリは FIFO 上限で自然に押し出される（`SPEC.md` §3.4）
- **`Skip` 行は placeholder へ落ちる**（`draw_icon_fallback`）＝現在と同じ絵
- **`is_error` 行にアイコンを出さない不変条件は、`wanted_icon_keys` の `!r.is_error` 条件だけが守っている。** `draw_result_row` の `Some(tex)` 枝（`results_view.rs:352-364`）に `is_error` ガードは無く、エラー行に絵が出ないのは「抽出要求に載らないので `icon_textures` にキーが無い」ことだけが理由である。しかも `folder::error_result`（`snotra-core/src/folder.rs:211-218`）は `path` に**実在ディレクトリの絶対パス**を入れ、発火点は `read_dir` の失敗（同 `:33`）——権限不足なら**ディレクトリは実在する**ので `SHGetFileInfoW` は成功する。**`!r.is_error` を落とすと、フォルダ列挙失敗行に本物のフォルダアイコンが描かれる**（plan-review B-1・再照合済み）。ゆえに `wanted_icon_keys` は `is_error` を読める形にし、専用テストを置く
- **展開後の exe が実在しないとき**は現状と同じ経路（`ShellQueryFailed` は `is_transient() == true` ゆえ `ICON_MAX_ATTEMPTS`=3 で打ち切り）
- **`expand_env` で未定義の `%VAR%` は字面のまま残る**（`ExpandEnvironmentStringsW` の意味論・要確認 → 未確定 U3）。その場合キーは実在しないパスになり、上の異常系と同じ処理へ落ちる
- **同一 exe を持つコマンドが複数**あっても、`wanted` の重複排除が既に効く（`results_view.rs:183`）

### 受容する費用（額は測っていない）

instant 枝は**毎打鍵** `matching_results` を呼ぶ（`launcher_controller.rs:910` の「毎打鍵同期」）ので、exec 型コマンド 1 件につき `ExpandEnvironmentStringsW` の呼び出しが 1 回増える。**額は未測定であり、「無視できる」とは書かない。** 言えるのは下限だけである——同じ関数が同じ打鍵で、マッチした全行ぶんの `name` / `path` の `String` を確保している。実 config・既定 config には exec 型が 0 件なので（research.md F11）、**現状この費用は 0 である**。

### 実運用点への届き方が非対称であること（research.md F11）

- 「URL / Legacy はスキップ」は**実 config と既定の両方に直接効く**（今日、`https://github.com/search?q={query}` のような文字列に対して `SHGetFileInfoW` が呼ばれ続けている）
- 「exec は本物のアイコン」は**exec 型を設定した利用者にしか届かない**（実 config・既定とも 0 件）

**仕様の姿は変えない**（仕様は 1 つの config ではなく製品の姿を定める）が、**検証労力を exec 側へ厚く配分しない**。実機での絵の確認を行わない判断（U5）はこの非対称とも整合する。

## テスト方針と検証コマンド

### 追加するテスト

**snotra-core（`instant.rs`）**——`no_env` ヘルパ（`:515`）の既存 idiom を使う

- `Url` → `Skip` / `Legacy` → `Skip`
- `Exec { args 空 }` → `Explicit(exe)` / `Exec { args 有 }` → `Explicit(exe)`（**`path` は `"exe args"` のまま**＝表示は変えない）
- `description` 非空 + `Exec` → `path` は description・`icon` は `Explicit(exe)`（**issue に無かった合流条件**・research.md F2）
- `env_expand` が `Explicit` の中身にだけ当たる（`path` には当たらない）

**snotra-core（`ui_types.rs`）**

- `icon_key()` の 3 variant（`FromPath` → `Some(path)` / `Skip` → `None` / `Explicit(k)` → `Some(k)`）

**src-tauri（`icon_textures.rs`）**

- `wanted_icon_keys`: `Skip` 行が 1 件も載らない / `Explicit` 行はそのキーで載る / `have`・`attempts`・`pending` の除外が効く / 重複が畳まれる / **`is_error` 行は `FromPath` でも `Explicit` でも載らない**（B-1。`path` が実在ディレクトリでも載らないことを、その形の行で測る）
- `visible_icon_keys`: `Skip` 行がセットに入らない
- `icon_for_row`: `Explicit` 行は `path` ではなくキーで引く（**`path` で引く変異が落ちる**）

**src-tauri（`results_view.rs` の `mod tests`・Phase 0a で先に置く）**

`input_idle` の意味論とゲートの位置を**ソーステキストで**固定する。先例は `launcher_controller.rs:1955` の `activation_entry_points_consult_the_display_gate`（#1077）で、**述語のテストでは呼び出し点の脱落を捕まえられない**という同じ理由に立つ。

```rust
#[test]
fn icon_gate_keeps_input_idle_semantics() {
    let view = include_str!("view.rs");
    assert!(view.contains("let input_idle = !self.controller.is_search_armed();"), ...);
    let this = include_str!("results_view.rs");
    assert!(this.contains("if snapshot.input_idle {"), ...);
}
```

- **存在形の assert だけで書く**（否定形は母集団が消えたときに沈黙する）。`include_str!` の母集団は実ファイル全体なので空になりえない
- **これが落ちたとき失うもの**: instant のスキップを `input_idle` 側で表現すると、**worker 走査中のアイコン取得が遅れる退行**が入る（`results_view.rs:36` の doc が「同じ修正を当ててはならない」と名指ししている）。しかも**絵は正しく見える**ので挙動テストでは捕まらない
- **残る死角（2 つ）**:
  1. 測っているのは部分文字列一致であって呼び出しではない。ゲートを別ヘルパーへ移して本体に綴りが残る形は緑のまま通る
  2. **より踏みやすい迂回がある**——`view.rs` が呼ぶ `is_search_armed()`（定義は `launcher_controller.rs:193-195` の 1 か所）の**中身**へ `|| instant_rows_query().is_some()` を仕込めば、#1074 が禁じた修正と同じ効果を、assert が見ている `view.rs` の 1 行を**1 文字も変えずに**達成できる。検知器は緑のまま通る。**doc にこの死角を宣言する**（実装での追加対処はしない——`is_search_armed` 側まで綴りで縛ると、正当なリファクタリングまで赤にする）

### 変異注入（検知器が効くことの実測）

各変異を当ててテストが**落ちる**ことを確かめ、戻す。**6・7 は Phase 0a で、1〜5 は Phase 2 で測る。**

1. `icon_for_row` を `icons.get(&result.path)` へ戻す
2. `visible_icon_keys` を `rows.iter().map(|r| r.path.clone())` へ戻す
3. `wanted_icon_keys` の `Skip` 判定を外す
4. `matching_results` の `Url` 枝を `FromPath` にする
5. `wanted_icon_keys` の `!r.is_error` 条件を外す（B-1 の回帰を再現する変異）
6. `view.rs` の `input_idle` の導出へ instant の条件を足す（`&& self.controller.instant_rows_query().is_none()`）——**#1074 が名指しで禁じた「同じ修正」そのものの形**
7. `results_view.rs` の `if snapshot.input_idle` を外す（ゲートの消滅）

## 実機確認（ユーザーの同意あり・2026-08-18）

**利用者の同意を得たので実施する。** ただし利用者の実インスタンスと実プロファイルには触れない。

### 前提と隔離

- **実行前に `snotra.exe` が動いていないことを確認する**——動いていると `tauri_plugin_single_instance` が 2 つ目のプロセスを黙って落とし（exit 0 / stderr 0 行）、代わりに**利用者のウィンドウがトグルされる**。プローブ側は検知できるが防げない
- `SNOTRA_CONFIG_DIR` を `target/probe-instant-icon/profile` へ向ける。**`%APPDATA%\Snotra` は読みも書きもしない**（`scripts/smoke-egui.ps1` が同じ隔離を既に実装しており・:47, :685、その形を借りる）
- **プローブスクリプトはリポジトリに残さない**（scratchpad で実行する）。残す足場には撤去条件が要り、その撤去条件は自己参照しやすい（memory `scaffold-removal-condition-self-reference`）——測定値だけを本計画と PR 本文へ残す

### プローブ用 config（3 件）

| 種別 | 内容 | 見るもの |
|---|---|---|
| url 型 | `description` に一意な番兵文字列 | A 側で `icon:extract_failed` に現れる／B 側で現れない |
| exec 型・args 空 | `exe = 'C:\Windows\System32\notepad.exe'` | B 側でアイコンが描かれる |
| exec 型・args 有 | 同じ exe ＋ `args = "{query}"` | B 側でアイコンが描かれる（**今日は失敗する経路**） |

**exe のパスは TOML の literal string（`'...'`）で書く**か、`\\` でエスケープするか、フォワードスラッシュへ変換する。**素の `"C:\Windows\..."` は無効エスケープ（`\W` / `\S`）で parse に失敗する**——リポジトリは既に 2 通りの回避を持っている（`snotra-core/src/config.rs:1985` の `"C:\\Tools"`、`scripts/smoke-egui.ps1:102` の `-replace '\\', '/'`）。

### 沈黙する 2 つの失敗を、肯定的な確認で塞ぐ

**どちらも「B 側で番兵が現れない」という期待結果と見分けがつかない**——失敗したまま「合格」になる。ゆえに**アームごとに肯定的な確認を先に置く**。

1. **config が実際に読まれたことを確認する** — parse に失敗すると `.bak` へ退避して**既定値（url 型 `g` / `gh` の 2 件）で起動**し、exec 型のプローブ 2 件は**存在しないまま**になる。判別子は行数である: `@` 1 打鍵で **instant 行が 3 件**出れば我々の config が載っている（既定なら 2 件）。`smoke-egui.ps1:309-327` と同型の `[config]` 診断行が出ていないことも併せて見る
2. **trace が実際に出ていることを確認する** — trace は `SNOTRA_TRACE` で開く（`scripts/smoke-egui.ps1` ほかが使う）。有効化を忘れると trace は 0 行になり、それは「番兵が無い」と同じ見た目になる。**A 側・B 側のどちらでも出るはずのイベント**（`hotkey:registered` 等）が実際に出ていることを、番兵を数える前に確かめる

### 手順

- **`@` の打鍵は VK の合成が要る**（US 配列なら Shift+2）。既存の `Get-LetterVk` は A〜Z しか扱わないので、`smoke-egui.ps1` の `:`（Shift+`;`）と同型の 1 行を足す
- **打鍵は 1 アーム 1 打鍵**（`@` のみ）。§19.5 の「プレフィックスだけの入力は全件表示」ゆえ、2 打鍵にすると 1 打鍵目で `egui_results:show` のエッジが出てしまい、2 打鍵目では二度と出ない非決定的な検査になる
- **A 側（Phase 0b・実装前）**: 番兵が `icon:extract_failed` に現れることを確認する（**陽性側は既存 trace だけで決定的**）。同時に「打鍵 → 失敗 trace」までのレイテンシ L を採る
- **B 側（Phase 5・実装後）**: 同じ手順で番兵が**現れない**ことを、予算 `max(B0, 5L)` で確かめる。**陰性の有効性は同一プロセス内の対照が担保する**——exec 行にアイコンが描かれることが「抽出経路は生きている」の証拠になるので、issue が設計した「実体を消したファイルを平文検索する対照アーム」は要らない
- **絵の確認は窓キャプチャで行う**（memory `debug-visual-render-precise-symptom`）。trace の集計は `$_.data.<name>` を読む（memory `keystroke-harness-failure-modes`）

### 検証コマンド（`docs/build-commands.md` カテゴリ A〜F から該当）

- `cargo test -p snotra-core` / `cargo test -p snotra`（A/B）
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo doc` （hook が走らない・memory `partial-automation-habituates`）
- `npm run governance:check`（F・`SPEC.md` と `docs/` を触るため）
- `/persistence-check`（`icons.bin` のキー空間が変わるため要否を判定 → 未確定 U4）
- `/dry-check`（型・関数の新規定義）

## `SPEC.md`・関連文書の更新要否

- **`SPEC.md` §3.4（正本）**: 「抽出の対象は行が指すファイルであり、キーは行ごとに定まる。既定は結果行の `path`。`path` がファイルを指さない行は抽出しない。別のファイルを指す行は、そのファイルのパスをキーにする」を追加
- **`SPEC.md` §19.5（写像）**: 当該行を「結果行のアイコン: exec 種別は env 展開後の `exe`、URL 種別は取得しない（§3.4）」へ置換。**規則の全文を写さない**（AGENTS.md「文書に事実の写しを増やす変更」）
- **`docs/architecture.md`**: 「path キーで stale 無害」の行を更新（この変更で偽になる）
- **`snotra-core/CLAUDE.md` / `src-tauri/CLAUDE.md`**: 新規ファイルを足さないのでモジュール索引の変更は無い
- **ADR**: `docs/adr/ADR-instant-row-icon-key.md` を新設する（U4 で決定）

## 作業項目

### Phase 0（実装より前・1 コミット）

- [ ] `results_view.rs` の `mod tests` に `icon_gate_keeps_input_idle_semantics` を追加する
- [ ] 現状のコードでその検査が**緑**であることを確かめる
- [ ] 変異 6（`input_idle` の導出へ instant の条件を足す）を当て、検査が赤になることを確かめて戻す
- [ ] 変異 7（`if snapshot.input_idle` を外す）を当て、検査が赤になることを確かめて戻す
- [ ] 実機ベースライン A 側を採る（`snotra.exe` の不在確認 → 隔離プロファイル → `@` 1 打鍵 → 番兵が `icon:extract_failed` に現れることの確認とレイテンシ L）

### Phase 1〜3（1 コミット）

- [ ] `snotra-core/src/ui_types.rs` に `IconSource` と `SearchResult.icon` と `icon_key()` を追加する
- [ ] `snotra-core` 内の `SearchResult` 構築点を移行する（compile-fail が尽きるまで）
- [ ] `snotra-core/src/instant.rs` の `matching_results` に `env_expand` を足し、種別 → `IconSource` を決める
- [ ] snotra-core の単体テストを追加する（上記「追加するテスト」の snotra-core 分）
- [ ] `src-tauri/src/egui_shell/icon_textures.rs` に `wanted_icon_keys` / `visible_icon_keys` / `icon_for_row` を新設する
- [ ] `results_view.rs` の 3 か所を新設した純関数へ寄せる
- [ ] `src-tauri` 内の `SearchResult` 構築点を移行する
- [ ] `launcher_controller.rs` の `matching_results` 呼び出しへ `expand_env` を渡す
- [ ] src-tauri の単体テストを追加する
- [ ] 変異 1〜5 を順に当て、対応するテストが落ちることを確かめて戻す

### Phase 4（1 コミット）

- [ ] `SPEC.md` §3.4 に規則の正本を追加する
- [ ] `SPEC.md` §19.5 の当該行を写像＋参照へ置換する
- [ ] `docs/architecture.md` の「path キーで stale 無害」の行を更新する
- [ ] 「path キー」と書いている**生きた** doc / コメント 3 か所を実装へ合わせる — `results_view.rs:570`（drain）・`results_view.rs:198`（`spawn_icon_load`）・`icon_textures.rs:8`（`IconMsg`）。**主張（staleness が構造的に無害）は変わらない**ので、変えるのは「path キー」→「アイコンキー」の語だけ
- [ ] `docs/adr/ADR-instant-row-icon-key.md` を書く（案 A・案 B の却下理由）

### Phase 5

- [ ] 実機 B 側を採る（A 側と同じ手順で番兵が現れないこと・exec 行 2 件にアイコンが描かれることを窓キャプチャで確認）
- [ ] `cargo test` / clippy / `cargo doc` / `governance:check` / `/dry-check` を実行し green を確認する
- [ ] `code-reviewer` の 3 フェーズレビューを通す

## 条件別チェックの適用（AGENTS.md「トリガー → 参照先」）

### 該当し、実施するもの

| トリガー | 参照先 | 本計画での扱い |
|---|---|---|
| 永続形式・識別子/キー形式を変更 | `/persistence-check` | 実行済み（下記「/persistence-check の結果」） |
| 関数・型を新規定義／導入 | findReferences + `/dry-check` | U2 で実施済み（`matching_results`）。`/dry-check` は Step 5a |
| どの分岐が選ばれるかを決める値の出所を変更 | 下流 1 段を辿り「この値で初めて走る行」を列挙 | 下記「新しく生きる組み合わせ」 |
| 重複した読みを束ねる | 各箇所について「後で読まれることに依存していないか」を 1 行ずつ | 下記「3 か所を束ねる根拠」 |
| ガバナンス文書を変更 | `npm run governance:check` | `SPEC.md` / `docs/architecture.md` を触るため Step 5 で実行 |
| 複数モジュール間のインターフェースを変更 | `/plan-review`（高リスク） | Step 2（計画準拠の独立レビュー）を 1 体 |

### 該当しないと判断したもの（根拠つき）

- **`/race-check`**: worker spawn・channel・listener・共有状態の**構造**を変えない。`RowsSnapshot` の中身（`SearchResult` のフィールド）は増えるが、差分判定は `cur_rows.as_slice() == rows`（`results_view.rs:59`）が `PartialEq` 経由で自動的に新フィールドを含む。フレーム内 live-read も増やさない（`input_idle` に触らない）
- **`/symmetric-check`**: 対称ペア（生成/破棄・show/hide）を作らない。`icon_pending` の挿入/削除は既存のまま
- **`/state-check`**: UI モード・ガード条件を増やさない（`plain_results_hidden` 等に触らない）
- **smoke の前提（trace イベント名・hotkey）**: 「表示経路の変更」トリガーに当たるので確認した——`scripts/smoke-egui.ps1` は **`icon` / 「アイコン」を 1 度も綴っていない**（grep 0 件・実測）。trace イベント名も hotkey 登録も触らないので、smoke の前提は壊れない
- **`/plan-review --deep`**: 網羅性は**コンパイラが持つ**（全構築点が compile-fail になる。件数は書かない——正本は E0063 であって人の数え上げではない）。母集団を人が数える種類の作業ではない

### 3 か所を束ねる根拠（「後で読まれることに依存していないか」）

1. **要求（`request_icons_for_results`）のキーは、そのまま worker へ渡り、`IconMsg::Loaded(path, ..)` で戻って `icon_textures.insert(path, ..)` のキーになる**（`results_view.rs:582`）。ゆえに引き側と同じ導出でなければ、抽出したテクスチャを**永久に引けない**
2. **引き（`results_list_ui` の `icons.get`）は 1 の結果だけを読む**。独自の導出を持つ理由が無い
3. **剪定（可視集合）は 1 と 2 の 3 つの map/set を保持する集合を決める**。1 と食い違えば、抽出した直後の世代交代フレームで drop され、次のフレームで積み直す往復になる

**drain（`icon_rx`・`results_view.rs:576-595`）は 4 か所目に見えるが導出点ではない**——worker へ渡したキーが返ってくるだけで、構造的に 1 と一致する。ただし当該行のコメント「path キーで適用」は文言が古くなるので直す。

### 新しく生きる組み合わせ（1 行も変えていないのに初めて走る下流）

- **instant の exec 行（args 有・description 有）で `IconOutcome::Png` が返る経路**——今日は `ShellQueryFailed` にしか到達しない。`icon_textures.insert` → `draw_result_row` の `Some(tex)` 枝 → `ui.painter().image(..)` が instant 行に対して初めて走る
- **`IconCache::insert` に実 exe パスがキーとして入る**——`icons.bin` の中身が変わる（FIFO 上限・退避経路は不変）
- **`Skip` 行で `wanted` が空になり `spawn_icon_load` が早期 return する**フレームが増える（`results_view.rs:188-190`・既存の枝だが instant では初めて常態になる）

## `/persistence-check` の結果（2026-08-18・実行済み）

**結論: version バンプ不要・後方互換テストの追加不要・データ保全は不変。永続化変更は安全。**

| Step | 判定 | 根拠 |
|---|---|---|
| 1 分類 | **どの分類にも当たらない**（形式変更・セマンティクス変更・シリアライザ切替・読込失敗ハンドリング変更のいずれでもない） | `IconCacheData`（`src-tauri/src/icon.rs:29-31`）に触れない。`SearchResult` は**永続形式に入らない**（`snotra-core/src/ui_types.rs:5-10` の `//!` が #836 の実測として記録） |
| 2 version | **バンプ不要**（`ICON_VERSION = 5` のまま・`icon.rs:25`） | バイト形式もキーの**解釈**（「このパスから抽出した PNG」）も不変。変わるのは instant exec 行に対して新たに現れる**キーの値**であって、既存エントリの意味ではない |
| 3 後方互換 | **既存テストで足りる。新規の凍結バイト列テストは不要** | キーは任意の `String` ゆえ旧 `icons.bin` はそのまま読める。旧キー（display 文字列）のエントリは**引かれなくなるだけ**で、デシリアライズは失敗しない |
| 4 データ保全 | **不変**（本変更の射程外） | `load` の `unwrap_or_default()`（`icon.rs:59-61`）も `save_if_dirty` も触らない |
| 5 移行経路 | **該当なし** | IndexCache 系・history 系・`Config` の TOML いずれにも触れない（`InstantCommand` の形は変えない） |
| 6 境界条件 | **追加不要** | 形式に関する境界条件は増えない |

**受容する残余**: 旧キー（instant 行の display 文字列）のエントリは、`cap` 件を上限に残りうる。これは `IconCache` の doc（`icon.rs:34-46`）が既に正本として記録している「索引に無いキーは `cap` 件を上限に残りうる——それは受容する」と**同型**であり、新しく書き足す規範は無い。

## 未確定（実装前に潰す）

- [x] U1. 抽出キーを読む箇所は本当に 3 か所か — **3 か所で全数**（敵対枠が `egui_shell/` 全体 + `icon.rs` + `commands/icon.rs` を横断して独立に数え上げ、反証できず）。drain と `load_icon_pngs` は下流の消費点であって導出点ではない。`platform/tray.rs` の `path` はアイコン抽出に使わない。`snotra-settings` / `snotra-egui-runtime` は `SearchResult` を参照しない
- [x] U2. `matching_results` の crate 外の呼び出し元 — **1 か所だけ**（`src-tauri/src/egui_shell/launcher_controller.rs:927`）。LSP `findReferences` で 10 件中 9 件は `snotra-core/src/instant.rs` 内のテスト。`snotra-settings` からの参照は無い。シグネチャ変更の影響は 1 点に閉じる
- [x] U3. `ExpandEnvironmentStringsW` を `read_config` の read guard 内で呼んでよいか — **呼んでよい。`matching_results` に `env_expand` 引数を足す形を採る**。`expand_env`（`src-tauri/src/commands/launch.rs:226-249`）は `ExpandEnvironmentStringsW` 1 発でプロセスの環境ブロックを読むだけで、**錠も取らず I/O もせず、不定時間ブロックしない**（`AppState::read_config` の doc・`src-tauri/src/state.rs:82-99` の禁止 3 種のいずれにも当たらない）。**却下した代替**: 展開を guard の外（呼び出し元）で当てる案は、忘れると**アイコンが黙って出ないだけ**の沈黙する欠陥になり、文書契約でしか守れない（ルート `CLAUDE.md`「表現不能にする」）。引数で受け取れば忘れようがない。既存の同型は `expand_exec_args(args, query, clipboard, env_expand)`（`snotra-core/src/instant.rs:296-308`）
- [x] U4. ADR を起こすか — **起こす**（`docs/adr/ADR-instant-row-icon-key.md`）。案 A（全スキップ＝SPEC の字面）と案 B（as-built 追認）を却下した理由は**否定の知識**であり、`docs/adr/` の設置基準（#593）に当たる。**issue と PR 本文では足りない**——PR 本文は squash で commit message になるがリポジトリの grep 母集団に入らず（memory `pr-body-is-outside-the-grep-population`・#1056）、issue はローカルに無い。「なぜ URL 行だけスキップなのか」を将来コードから辿れる先が要る
- [x] U5. 実機確認を行うか — **行う（ユーザーの同意を 2026-08-18 に取得）。** 設計は上の「実機確認」節。受け入れ条件 2・3 の**機構**は純関数の単体テスト＋変異注入が決定的に測るので、実機が担うのは**絵**（exec 行に本物のアイコンが描かれること）と、番兵による陽性 → 陰性の遷移である。当初は「同意なしに利用者の実インスタンスをトグルしない」を理由に見送っていた

## plan-review 結果

- リスク: **高**（複数モジュール間のインターフェースを新設・変更する——`snotra-core` の `SearchResult` / `matching_results` を `src-tauri` が消費する）
- レビュー方式: **計画準拠レビュー 1 体**（Step 2。Step 2b の独立導出は採らない——網羅性はコンパイラが持つため）
- エージェント数: **1**（本サイクル全体では 2 体。もう 1 体は Step 3b の敵対的調査）
- 成果物: `workspace/plan-review-instant-icon-key.md`

### 要対処

- **B-1. `wanted_icon_keys` が `is_error` を落とすと、フォルダ列挙失敗行に本物のフォルダアイコンが描かれる** — 計画を修正（不変条件の節に機序を追記・テスト 1 件追加・変異 5 を追加） — **再照合の根拠**: `folder::error_result` は `path` に実在ディレクトリの絶対パスを入れ（`snotra-core/src/folder.rs:211-218`）、発火点は `read_dir` の失敗（同 `:33`）なので**権限不足ならディレクトリは実在する**。`draw_result_row` の `Some(tex)` 枝（`src-tauri/src/egui_shell/results_view.rs:352-364`）に `is_error` ガードは無く、現在エラー行に絵が出ないのは抽出要求に載らないことだけが理由。3 点とも一次証拠で確認した

### 軽微

- **A-1. 変更ファイル一覧の `engine.rs` / `index_tree.rs` は構築点を持たない** — 一覧を実測へ差し替えた（`folder.rs` / `instant.rs` / `search.rs` / `search/scoring.rs` / `search_state.rs` / `results_view.rs` / `tray.rs`）。実害は無い（コンパイラが実物を強制する）
- **A-2. 「構築点 23 か所」は `grep -c` の生値で不正確** — **件数の訂正ではなく件数の削除で直した**。3b は「19 リテラル + 関数シグネチャ 2 + 定義 1」、plan-review は「22 リテラル」と数え、**同じ grep から食い違う数が出た**。数え上げは足すたびに腐るので、正本（`-D warnings` 下の E0063）を指す形へ変えた（`AGENTS.md`「数え上げも同じ強さである」）

### 未検証

- なし（レビュア自身の宣言も「未検証: なし」）

### 判断

- 実装着手: **人間の裁定待ち**（未確定欄は空・受け入れ条件とテスト期待値は Q1 の回答で変わらないため plan-review の再実行は不要）

## セルフレビュー

- リスク: **高**
- plan-review: **独立レビュー 1 体**（Step 2・計画準拠）
- エージェント数: **1**（3b の敵対枠を含めると本サイクル 2 体）
- 要対処: **1 件**（B-1）。計画へ反映済み——不変条件に機序を追記・`wanted_icon_keys` の `is_error` テストを追加・変異 5 を追加。軽微 2 件も反映済み
- 未検証: **なし**（当初は「`RowsSnapshot::input_idle` に触らないこと」に検知手段が無く差分レビュー頼みだったが、**ユーザー注釈により検知器を Phase 0a で先に置く**ことにした。理由も同じ——レビュアは落とすときは落とす）

### ユーザー注釈による差分（2026-08-18）

1. **実機確認を行う**（同意取得）→ U5 を反転し「実機確認」節と Phase 0b / Phase 5 の作業項目を追加
2. **`input_idle` の検知器を実装より前に置く** → Phase 0a を新設し、検知器・変異 6・7・作業項目 4 件を追加

**テスト期待値とフェーズ構成が変わった高リスク計画**なので、`/plan-review` を**この差分に限って**もう 1 回実行した。

### 追加 plan-review（差分・`workspace/plan-review-delta-detector.md`）

- 観点: C（検知器は本当に効くか）/ D（実機手順に実行時の判断が残っていないか）
- エージェント数: 1（本サイクル通算 3 体）

**要対処 2 件（いずれも反映済み・再照合済み）。2 件は同じ形の欠陥である——失敗が、期待する成功と同じ見た目になる。**

- **D-1. プローブ config の exe パスが TOML として parse できない** — `"C:\Windows\System32\notepad.exe"` は `\W` / `\S` が無効エスケープ。parse に失敗すると `.bak` 退避 → **既定値（url 型 2 件）で起動**し、exec 型プローブが存在しないまま実機確認が「合格」する。**再照合の根拠**: `snotra-core/src/config.rs:1985` が `"C:\\Tools"` とエスケープし、`scripts/smoke-egui.ps1:102` が `-replace '\\', '/'` で回避している（リポジトリ自身が 2 通りの回避を持っている）。**対処**: literal string へ変更し、「instant 行が 3 件出ること」を config 到達の判別子として手順へ追加
- **D-2. B 側の陰性判定が `SNOTRA_TRACE` の有効化を暗黙の前提にしている** — 忘れると trace が 0 行になり、「番兵が無い」と区別が付かない。**再照合の根拠**: `SNOTRA_TRACE` は環境変数で trace を開く（`scripts/smoke-egui.ps1` ほか多数が設定している・grep 実測）。計画が借用を明記していたのは `SNOTRA_CONFIG_DIR` だけだった。**対処**: 番兵を数える前に、A/B どちらでも出るはずのイベントが出ていることを確かめる手順を追加

**軽微 3 件（反映済み）**: 検知器の残る死角に `is_search_armed()` の中身を書き換える迂回を追記／`@` の VK 合成が要ること／`include_str!` のクロスファイル使用はリポジトリ初だが技術的問題は無い（既存先例はすべて自ファイル参照という差だけ）。

**未検証**: なし

## 人間レビュー

- [x] 承認済み — 2026-08-18 / 問い: "`workspace/plan.md` へ注釈を書き足していただくか、**「承認」**とお伝えください。承認後に `workspace/` をコミット・push し、`/implement` へ渡せる状態にします。それまで実装には入りません。" / 回答: "1. OK / 2. 検知器の死角を塞ぎ切らない了解。ようは、必要な分だけ縛るわけね / 3. プローブ撤去もOK、使いまわさないなら撤去が賢明 / 計画全体を承認"

**承認時の注釈（設計判断の言い換え・そのまま残す）**: 検知器は「**必要な分だけ縛る**」。広く縛るほど強くなるのではなく、**正当な変更まで赤にした瞬間に無視されるようになる**——だから `is_search_armed()` の中身までは綴りで縛らず、死角として宣言するに留める（上の「残る死角 2」）。

### 承認前に確定させる要求判断（ユーザーへの問い）

- **Q1（回答済み）**: exec 種別で `description` が設定されている行の扱い → **「exec なら常に exe のアイコン」**。計画の版と一致するため、受け入れ条件もテスト期待値も変わらない
- **Q2（回答済み・2026-08-18）**: 敵対枠の実測（exec 型 instant コマンドは実 config・既定 config とも 0 件）を踏まえた問い直し → **「C のまま確定」**。仕様は 1 つの config ではなく製品の姿を定めるため、届き方の非対称は設計判断を変えない。計画は現在の形のまま

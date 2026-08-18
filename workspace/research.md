# 調査 — #1129 AppState.config の直読みを塞ぐか（private 化が先か、検知器か）

ブランチ: `chore/appstate-config-private`

## issue の要約

#1123 は config の読みを `AppState::read_config` の 1 か所へ集約し、`Engine::config` を
`#[cfg(test)]` で閉じて engine 錠越しの読みを**コンパイラが禁じる**形にした。残った経路が 1 つある——
`AppState.config` は `pub` のままなので `state.config.read().unwrap()` と直に書けばコンパイルが通り、
`read_config` の契約（read guard の中で錠も I/O も取らない）を素通りする。

issue が決めたいのは**この残余に検知器を置くか**であり、**判断の順序を issue 自身が課している**:

1. まず `AppState.config` を private にできるかを測る（構築点の列挙。通るなら検査は不要）
2. 通らない場合にだけ検査の是非を評価する。そのときも**発火しうるかを先に測る**

## 測定 1 — `AppState.config` を private にできるか（→ できる）

### 構築点は 3 件（`AppState { .. }` 構造体リテラル）

| 位置 | 種別 | `indexing` の初期値 |
|---|---|---|
| `src-tauri/src/main.rs:240` | 製品 | `initial_indexing`（変数） |
| `src-tauri/src/commands/system.rs:44` | `#[cfg(test)]`・**別モジュール** | 引数 `indexing: bool` |
| `src-tauri/src/state.rs:110` | `#[cfg(test)]`・**同一モジュール** | `false` |

**3 件とも形が同一である**——`config: engine.config_handle(), engine: Mutex::new(engine)` と書き、
残る 4 フィールドは `AtomicBool::new(..)` / `AtomicU64::new(0)`。違うのは `indexing` の初期値だけ。

### `config_handle()` の呼び出し点は、この 3 件の構築だけである

`grep -rn "config_handle" --include=*.rs .`（フィルタ無しでも同じ・実測）:
定義は `snotra-core/src/engine.rs:267`、呼びは上記 3 件のみ。残りは doc コメントでの言及 3 件。

### `.config` へのドット参照は `state.rs:75` の 1 行だけ

`grep -rn "\.config\." src-tauri/src/`:
`src-tauri/src/state.rs:75: read(&self.config.read().unwrap())` ——`read_config` の本体。
製品コードで `state.config` を触る箇所は**存在しない**（issue 本文の grep 実測と一致）。

### `src-tauri` は bin crate である（lib ターゲットが無い）

`src-tauri/src/lib.rs` は存在せず、`Cargo.toml` に `[lib]` も無い。ゆえに `AppState` は
**外部 crate から不可視**であり、可視性の検討は crate 内に閉じる。

### 列挙の完全性はコンパイラが担保する

上の grep が仮に取りこぼしても、`config` を private にした瞬間に**取りこぼした地点は
private field エラーでコンパイルが落ちる**。AGENTS.md「新 API の導入と呼び出し点の移行を
1 タスクに束ねる／compile-fail を移行漏れ検出器に」の形であり、**網羅性は grep に依存しない**。

## 測定 2 — 検知器は要るか（→ 評価に進まない。issue の順序 1 が通った）

issue の順序 2 は「1 が通らない場合にだけ」評価する。1 が通るので**検知器案は評価に進まない**。
記録として、検知器案が抱えていた費用（issue が挙げたもの）:

- 誤検出リスク: `Engine` 自身が `snotra-core` で `self.config.read()` を正当に書く（実測 9 行——
  `engine.rs:150,161,166,221,226,247,297,315` の read と 254 の write）。除外の書き損じは沈黙する
- 呼び出し点 0 件の残余への検知器（`measure-whether-detector-can-fire` の型。#930 は測ってクローズ）
- セーフティネットの新設（ルート `CLAUDE.md` 最重要ルール 2 — チームの合意が要る）

**private 化は検知器より上段である**（`docs/development-principles.md`「規範を書くときの作法」の
「構造が規則を吸収したら対応するチェックリストを削除する——ドキュメントが軽くなることを
設計改善の検収条件とする」）。

## 副産物 — #1032 の不変条件が規範から構造へ移る

`AppState::new(engine, initial_indexing)` へ集約すると、「`config` は engine が持つのと**同じ Arc**」
（#1032・写しではない）が**3 か所で正しく書く規範**から**1 か所の構造**になる。現在は構築点が
増えるたびに `engine.config_handle()` と書き忘れない規律が要る（別の `Arc` を渡してもコンパイルは通り、
UI と検索が違う config で動く）。これは issue が挙げていない、private 化に付随する 2 つ目の利得である。

## 変更後に残る残余（**ここを誤ると偽の全称になる**・#977 / #1091 の型）

private 化で閉じるのは**モジュール外から届く綴りだけ**である。次の 3 つは**閉じない**:

1. **`engine.lock().unwrap().config_handle().read()` は今も通る。** #1123 が 2026-08-18 に実測して
   条項へ書いた残余であり、本変更は**これに触れない**。新しい条項文へ「直読みは書けなくなった」と
   無条件に書いてはならない
2. **`state.rs` の中（`#[cfg(test)] mod tests` を含む）ではフィールドを綴れる。** #1123 の言い回しに
   倣い「閉じたのは外から届く綴り」と書く
3. **`read` へ渡すクロージャの中に錠や I/O を書く形は構造では止まらない**（既存の受容残余・不変）

## 関連ファイル・シンボル

| パス | 対象 | 役割 |
|---|---|---|
| `src-tauri/src/state.rs` | `AppState`（構造体・フィールド doc）/ `AppState::read_config` の doc / `mod tests` の `test_state` | 変更の中心。`new` の追加先 |
| `src-tauri/src/main.rs:240` | `app_state` 束縛 | 構築点（製品） |
| `src-tauri/src/commands/system.rs:42` | `test_state(indexing: bool)` | 構築点（test・別モジュール） |
| `src-tauri/CLAUDE.md:57` | 「config の読みは `read_config` を通す」条項 | 「**ただし機構ではない**（`AppState.config` は `pub` ゆえ直読みは通る）」が偽になる |
| `snotra-core/src/engine.rs:267` | `Engine::config_handle` | 呼び出し点が 1 か所へ減る（可視性は変えない——crate をまたぐため） |

## 文書の写しの母集団（**フィルタ無しの grep で測った**）

`grep -rn "AppState\.config\|state\.config\|AppState::read_config" .`（`--include` を付けない。
RETROSPECTIVE の「包含フィルタは除外句と違い『除外した』自覚が残らない」に従う）。

**生きた層で更新が要るもの（2 件）**:

- `src-tauri/src/state.rs:47`（`read_config` doc の「**ただし表現不能化ではない**」段落）と、
  同ファイルの `config` フィールド doc
- `src-tauri/CLAUDE.md:57`（「**ただし機構ではない**（`AppState.config` は `pub` ゆえ直読みは通る）」）

**凍結ゆえ編集しないもの（`ADR-adr-frozen-history`）**:

- `docs/adr/ADR-config-read-exception-discriminator.md:25`（案 G の却下理由「`Engine` から config を
  外へ出すのは既に限界まで済んでおり」——本変更でその一部が偽になるが、**凍結ゆえ直さず新 ADR へ書く**。
  #1123 が同じ扱いをした先例がある）
- `docs/adr/ADR-config-read-without-exception.md:45`（残余の列挙に「`AppState.config` の直読みが
  書けること」が入る）

**参照のみで更新不要**: `snotra-core/src/engine.rs:240`、`commands/instant.rs:10`、
`launcher_controller.rs:731`、`egui_shell/mod.rs:419,422`、`state.rs:150`（いずれも
`AppState::read_config` を**呼び口として**指すだけで、可視性には言及しない）。

**PR 本文も写しの母集団に入る**（squash で main の commit message になる・
`pr-body-is-outside-the-grep-population`）。

## 再利用できる既存パターン

- **#1123 の言い回し**: 「閉じたのは外から届く綴りであって、〜の内側は自分の `config` フィールドを
  直に読む」「**規範は機構より広い**」——射程を正確に書くための既製の型
- **#1123 のフォールトインジェクション**: 回帰の形を注入して `cargo check` の**エラーコードまで実測**し、
  日付とコードを doc へ書いた（`.claude/rules/safety-nets.md`「効いていることは、フォールト
  インジェクションで一度は実測する」）。本変更でも同じ作法を取る
- **`Engine::config` の `#[cfg(test)]` 閉包**（#1123）: 「読み口を 1 つに保つ」を可視性で守る先例
- **`IndexMaterial` のフィールド private 化**（`PERFORMANCE.md:1770-1778`・`snotra-core`）:
  **本件と同型の先例である**——「マージは 1 メソッドで、フィールドが private ゆえ**crate 外から
  片方だけ伸ばす形は書けない**」「**crate 内ではまだ書ける**が、呼び出し点は無い」と射程を切っている。
  同じ差分で「**『表現不能化ではない』という受容宣言は 4 か所から消した**」——受容宣言を消すのは
  実態が変わったときだけ、という作法もそこにある

## 技術的制約

- **Rust の構造体リテラルは全フィールドが可視でないと書けない**。`config` を 1 つ private にすれば、
  他モジュールの構築は不可能になり `new` が必須になる（これが移行漏れ検出器の実体）
- **`Mutex::new(engine)` は `engine` をムーブする**ので、`new` の中では `config_handle()` を**先に**呼ぶ
- **`pub(crate) fn new` に `dead_code` は立たない**（`main.rs` から呼ばれる）。RETROSPECTIVE が記録した
  「`pub(crate)` では `dead_code` が立って実装が止まる」は**未使用**の項目に起きる話で、本件は当たらない
- **Tauri の `.manage()` 要件は変わらない**（`AppState: Send + Sync + 'static`。可視性は無関係）
- **`main_visible` 等の他フィールドは `pub` のまま**——それぞれ別に受容残余として doc 化されており、
  射程を広げない（issue の射程は `config` のみ）

## 敵対的調査（3b・sonnet 1 体）の結果と採否

出力: `workspace/adversarial-1129.txt`。争点 7 件を「偽にできる命題」の形で渡した。

**壊せた項目: 0 件。** 争点 1〜7 のすべてと、測定環境（grep のフィルタ差・cfg 分岐・`clippy.toml`・
cargo のターゲット構成）を疑う指示に対して、`research.md` の主張は一次証拠と一致した。相手が実行した
検算のうち本調査を補強するもの: フィルタ無し grep での構築点 3 件・`config_handle` 3 件の再測定、
他 3 crate に `AppState` の出現 0 件、`[lib]`/`[[test]]`/`[[bench]]`/`[[example]]` の不在、
`src-tauri/tests/` 自体の不在、`State<AppState>` / `try_state` の 27 件が**すべて不透明取得で
`.config` のドット参照を伴わない**こと、derive / serde 実装 / `macro_rules!` / doctest 経路の不在。

**新たに出た所見 3 件と採否**:

1. **採用**——`AppState {` はフィルタ無し grep だと `.superpowers/sdd/*.diff` と
   `docs/superpowers/plans/*.md`（過去の計画・レビュー diff）にも現れる。**現行コードではないので
   構築点の数は変わらない**が、**私の grep が `src-tauri/src/` に限っていたためこれらは見えていなかった**。
   ヒットが出た場合に「現行コードか過去の diff か」を判別する必要があるという事実を記録する
2. **採用**——issue 本文の「7 か所」と本調査の「9 行」は粒度差である（247 行の `#[cfg(test)]`
   アクセサと 254 行の write を数に入れるか）。**矛盾ではない**が、PR 本文で一言断ると誤読を防げる
3. **採用（機序は自分で裁定した）**——フォールトインジェクションは相手の権限外（ファイル改変を禁じたため）。
   **下の「実測」節で私が自分で実施した**

## 実測 — 機構が効くこと／効かないことを同じ場で測る（2026-08-18・**未確定の解消**）

`state.rs` の `pub config` から `pub` を外し（`new` は実装せず）、`commands/system.rs` の
`#[cfg(test)]` 内へ probe を注入して `cargo check --workspace --all-targets` を実行、直後に
`git checkout --` で復元した（**ガードの行使であって弱めていない**——`.claude/rules/safety-nets.md`
「意図的に規則違反となる操作を行い、拒否されることを確認する類は対象外」）。

| 注入した綴り | 結果 |
|---|---|
| `state.config.read().unwrap()`（別モジュール） | **E0616** `field 'config' of struct 'state::AppState' is private`（`commands/system.rs:44`） |
| `AppState { config: engine.config_handle(), .. }`（別モジュール） | **E0451** `field 'config' of struct 'AppState' is private`（`main.rs:242`） |
| `state.engine.lock().unwrap().config_handle().read().unwrap()` | **可視性のエラーは 1 つも出ない**——**今も通る** |

3 行目は分離して測り直した（1 行目を消して再度 `cargo check`）。出たのは
`error: non-binding let on a synchronization lock`——**probe の `let _ =` という書き方に対する
rustc の lint であって、可視性による拒否ではない**。#1123 が 2026-08-18 に測った残余は本変更後も
そのまま残る。

## 未解決の疑問

**なし**（上の実測で解消）。新 ADR を書くかは計画側で決定した（**書く**——検知器を置かない判断は
否定の知識であり、`ADR-config-read-without-exception` が残余として記録した事実が本変更で
偽になるが、凍結ゆえあちらを直せないため受け皿が要る）。

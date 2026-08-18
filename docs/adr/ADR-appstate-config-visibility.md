# ADR-appstate-config-visibility: config の読み口を検知器ではなく可視性で守る

日付: 2026-08-18 ／ 状態: 承認

## 文脈

#1123 は config の読みを `AppState::read_config` へ集約し、`Engine::config` を `#[cfg(test)]` で閉じて、
engine 錠越しの読みをコンパイラが禁じる形にした。**残った経路が 1 つあった**——`AppState.config` が
`pub` のままだったので、`state.config.read().unwrap()` と直に書けばコンパイルが通り、`read_config` が
課す契約（read guard の中で錠も I/O も取らない）を素通りできた。同サイクルの `/simplify` はここへ
ガバナンス検査（`\.config\.(read|write)\(` を `src-tauri/` で走査し `state.rs` 以外を落とす）を置く案を
挙げ、#1129 が「置くか」を問うた。

**issue は判断の順序を自分で課していた**: まず `AppState.config` を private にできるかを測り、
**通らない場合にだけ**検査の是非を評価する。構造で塞げるなら検査は要らないからである。

## 決定

**`AppState.config` を private にし、`pub(crate) fn AppState::new(engine, initial_indexing)` を
唯一の構築点にする。検知器は置かない。**

順序 1 を測った結果は「通る」だった——構築点は 3 件（`main.rs` の 1 件と `#[cfg(test)]` の 2 件）、
`Engine::config_handle()` の呼び出し点はその 3 件と同一、`.config` へのドット参照は `read_config` の
本体 1 行のみ、`src-tauri` は `[lib]` を持たない bin crate ゆえ外部 crate から `AppState` は見えない。
**ゆえに順序 2（検査の是非）へは進んでいない。**

副産物として、#1032 の「`config` は engine が持つのと同じ `Arc` である」を**正しく書く地点が 3 か所から
1 か所へ縮んだ**。別の `Arc` を渡してもコンパイルは通るので、構築点が増えるたびに書き忘れない規律が
要る形だった——**その規律が消えたのではなく、置き場が 1 つになった**（測るのは既存テスト
`app_state_config_is_the_same_arc_the_engine_holds` である）。

## 却下した代替案

- **案 1: ガバナンス検査（grep 検知器）を置く**: 却下。**階梯の下段だからである**——`AppState.config` を
  private にすれば同じことを型が保証し、検査は不要になる（`docs/development-principles.md`「規範を書く
  ときの作法」の「構造が規則を吸収したら対応するチェックリストを削除する——ドキュメントが軽くなる
  ことを設計改善の検収条件とする」）。加えて費用が 3 つあった。(1) **誤検出リスクが実在する**——
  `Engine` 自身が `snotra-core` で `self.config.read()` を正当に書いており（read 8 行 + write 1 行）、
  `state.rs` の `#[cfg(test)]` も読む。除外の書き損じは沈黙で射程外へ落ちる（撤去した `G-clippy-disallowed`
  群 3 のコメントが「名指したパスが解決することは見ない」と警告していた形）。(2) **呼び出し点 0 件の
  残余に検知器を置くことになる**（`measure-whether-detector-can-fire` の型。#930 は測った結果クローズ
  した）。(3) **セーフティネットの新設**ゆえチームの合意が要る（ルート `CLAUDE.md` 最重要ルール 2）。
- **案 2: `config` だけ private にして `new` を作らない**: 却下——**成立しない**。Rust の構造体リテラルは
  全フィールドが可視でないと書けないので、`main.rs` と `commands/system.rs` の構築が不可能になる。
  この不成立は欠点ではなく**移行漏れ検出器の実体**である（取りこぼした地点は E0451 で落ちる）。
- **案 3: `AppState` の全フィールドを private にして accessor を並べる**: 却下。`engine` /
  `indexing` / `main_visible` / `index_generation` は crate 内から直に読み書きされており、
  **射程を広げるだけで守るものが無い**。とくに `main_visible` は「`pub` ゆえ crate 内のどこにでも
  `store()` を書ける」ことを**受容する残余として自分の doc に明記している**——閉じるなら
  `.hide()` の生の面（`Manager` から引ける）も同時に塞ぐ必要があり、可視性だけでは片付かない。
  #1129 の射程は `config` である。
- **案 4: `Engine::config_handle` の可視性を狭める**: 却下——**crate をまたぐため不可能**。
  `src-tauri` からの正当な呼び出し（`AppState::new` の 1 行）が残る。#1123 が `Engine::config` に
  対して `#[cfg(test)]` を使えたのは、そちらの読み手が snotra-core 内部だけになったからである。

## 旧 ADR との関係（凍結ゆえ直さず、ここへ書く）

**`ADR-config-read-without-exception`「帰結」が挙げた受容残余の 1 つが偽になった。** 同 ADR は
「残った受容残余（規範が機構より広いこと・guard 内の I/O は構造で止まらないこと・**`AppState.config` の
直読みが書けること**）は条項と `AppState::read_config` の doc が持つ」と書いている。3 つ目が本決定で
閉じた——ただし**閉じたのは `state.rs` の外から届く綴りだけ**であり、残り 2 つは今も真である。

**`ADR-config-read-exception-discriminator` の案 G（規範を型・構造で強制する）の却下理由が、もう 1 つ
偽になった。** 同 ADR は 3 つの理由で案 G を却下し、#1123 がそのうち 2 つを偽にしたうえで
「なお『`Engine` から config を外へ出すのは既に限界まで済んでいる』は今も真である」と留保していた。
**その留保が指していたのは「`Engine` から外へ出す」向きであって、「外へ出した先を閉じる」向きではない**
——後者にはまだ余地があり、本決定がそれを使った。凍結された ADR は直さない（`ADR-adr-frozen-history`）。

## 帰結

- **規則の現在の全文は `src-tauri/CLAUDE.md`「モジュール構成」の当該条項が正本である**——ここに写しを
  置かない。本決定が条項へ与えた変化は「読み口に加えて構築点も閉じたこと」であり、**残余の内訳**
  （規範が機構より広いこと・`state.rs` の内側では今も綴れること・guard 内の錠と I/O は構造で
  止まらないこと）は条項と `AppState::read_config` の doc が持つ。
- **機構は両向きに測ってある**（`.claude/rules/safety-nets.md`「効いていることは、フォールト
  インジェクションで一度は実測する」）。回帰の形を注入すると `cargo check` が **E0616**（直読み）
  / **E0451**（構築リテラル）で落ちる。**同じ場で反対向きも測った**——
  `engine.lock().unwrap().config_handle().read()` は可視性のエラーを 1 つも出さず**今も通る**。
  ゆえに「engine 錠越しの config の読みは書けなくなった」とは**依然として言えない**。
- **新しいテストは足していない。** 受け入れ条件を測るのはコンパイラであり、`#[cfg(test)]` の中では
  当のフィールドが綴れてしまうのでテストでは表現できない。既存の
  `app_state_config_is_the_same_arc_the_engine_holds` は、構築が集約された結果として
  **`AppState::new` を測るテストになった**。

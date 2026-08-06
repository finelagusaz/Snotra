# 実装計画 — issue #950: G-clippy-disallowed

## 目的

`src-tauri/clippy.toml` の禁止集合が**黙って空洞化する経路**を塞ぐ。手段は 2 つで、片方だけでは足りない。

1. **`governance:check` へ静的検査 `G-clippy-disallowed` を 1 本足す**（issue 本文の判定 A / B / C）
2. **ルート `[workspace.lints.clippy]` で `disallowed_methods` を deny 化する**（issue コメントの選択肢 2 =
   起票者判断で採択・2026-08-06）。`-D warnings` への依存そのものを消す構造的解決

2 を入れても 1 は要る——deny にしても `clippy.toml` が空なら禁止するものが無い（`research.md` 制約 3 で実測）。

## 受け入れ条件

- `npm run governance:check` が緑で、evidence に禁止 7 件が現れる
- 下表の 9 つの変異のそれぞれで、`G-clippy-disallowed` が**赤になる**（フィクスチャで実証）

| # | 変異 | 由来 |
|---|---|---|
| 1 | `clippy.toml` の削除 | issue 本文 |
| 2 | `disallowed-methods` 配列ごと消滅 | issue 本文 |
| 3 | 空配列化 | issue 本文 |
| 4 | エントリが 1 行だけ消える | issue 本文 |
| 5 | メソッド名の書き損じ | issue 本文 |
| 6 | crate 名の書き損じ（`eguii::`） | issue 本文 |
| 7 | `egui` 依存の消滅 | issue 本文 |
| 8a | ルートから `[workspace.lints.clippy]` 節ごと消える（**2 行削除＝最も起きやすい形**） | 選択肢 2 が新設するノブ自身の沈黙 |
| 8b | ルートの deny が `warn` へ降格 | 同上 |
| 9 | エントリが `#` でコメントアウトされる | 実測で発見（素朴な述語が緑で通す） |

- `cargo clippy -p snotra --all-targets`（**`-D warnings` 無し**）が、禁止メソッドの呼び出しに対して
  `error` を出し exit≠0 になる（`research.md` 制約 3 で実測済み。実装後に再実測する）
- 他 3 crate の clippy が無影響（exit 0）

## 変更ファイルと対象シンボル

| ファイル | 変更 |
|---|---|
| `Cargo.toml` | `[workspace.lints.clippy]` を新設し `disallowed_methods = "deny"`。`[workspace.lints.rustdoc]` の直後に置く |
| `scripts/governance-check.mjs` | 新設: `REQUIRED_DISALLOWED_METHODS` / `stripTomlComment` / `disallowedMethodPaths` / `declaresEguiDependency` / `clippyMethodsDenied` / `checkClippyDisallowed`。`buildChecks` へ `{ id: "G-clippy-disallowed" }` を登録。`evidence` へ禁止件数を追加。**`G-workspace-lints` 冒頭の「受容する残余」1 項を訂正** |
| `scripts/governance-check.test.mjs` | `describe("G-clippy-disallowed …")` を追加（緑 3 + 赤 10 相当）。`import` に新シンボルを追加 |
| `src-tauri/clippy.toml` | 冒頭コメントの「沈黙経路 0」段落を訂正（deny 化で `-D warnings` 依存が消えた）。末尾の「`[workspace.lints]` は使えない」を「**内容**は移せない／**レベル**は移した」へ鋭くする |
| `docs/build-commands.md` | 1 文追加（先例 `ca8afae` と同じ扱い）。clippy の禁止が `-D warnings` ではなく workspace lints の deny で効くこと、空洞化は `G-clippy-disallowed` が見ること |
| `docs/adr/ADR-clippy-disallowed-enforcement.md` | 新規。却下した選択肢 1（起動形の検査）と選択肢 3（記録に留める）の否定の知識 |

`SPEC.md` は**更新不要**（CI の静的検査であり製品の意図ではない。先例 `ca8afae` も触っていない・実測）。
`src-tauri/CLAUDE.md` も**更新不要**（`-D warnings` に言及していない・実測）。

## 実装順序

### Phase 1 — 構造で沈黙経路 0 を消す

- [ ] `Cargo.toml` へ `[workspace.lints.clippy]` / `disallowed_methods = "deny"` を追加（rustdoc ブロックの直後）
- [ ] 意図をコメント 2 行で添える（既定 warn ゆえ `-D warnings` に依存していたこと・`clippy.toml` を持たない
      3 crate は禁止集合が空で無害であること）
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` が緑であることを確認

### Phase 2 — 検査本体

- [ ] `stripTomlComment(raw)` を新設する。**`tomlLine` は使わない**——`reason` の中の `#751` で行が切れる（実測）
- [ ] `disallowedMethodPaths(text)`: コメント除去 → `disallowed-methods = [ … ]` の**配列内へスコープ** →
      `matchAll(/path\s*=\s*"([^"]+)"/g)`。配列そのものが無ければ `null` を返す（「空」と区別する）
- [ ] `declaresEguiDependency(text)`: セクション見出しを追い、`…dependencies]` 配下でのみ
      `^egui\s*=` / `^egui\.<key>\s*=` を見る。**字面一致にしない**——`snotra-egui-runtime = …` が誤爆する
- [ ] `clippyMethodsDenied(rootText)`: `[workspace.lints.clippy]` 配下の `disallowed_methods` の level が
      `deny` / `forbid`。文字列形とテーブル形の 2 形を受ける（`rustdocLintsAreDenied` と同じ形）
- [ ] `REQUIRED_DISALLOWED_METHODS`（7 パス）を `REQUIRED_RUSTDOC_LINTS` と同じ位置へ置き、
      **名指しが意図的である理由**（1 行消えた形が ∀ 条件を緑で通す）をコメントで弁護する
- [ ] `checkClippyDisallowed(snapshot)` を組む（findings クラスは下記）
- [ ] 検査ブロックの冒頭コメントを既存 `G-*` の様式で書く: 守る命題（前提つき）/ 塞ぐ経路 /
      射程外（`reason` 文言・`#[allow]` 迂回・cargo fingerprint によるキャッシュ replay）/ 受容する残余
- [ ] `buildChecks` へ `{ id: "G-clippy-disallowed", run: () => checkClippyDisallowed(snapshot) }` を登録
- [ ] `evidence` へ `clippy 禁止 N 件` を追加。**配列が無い/読めない場合の N は 0 とする**——
      `disallowedMethodPaths` は `null` を返すので、素直に書くと `clippy 禁止 undefined 件` になり、
      この検査が存在する当の失敗ケースで evidence が壊れる

### Phase 3 — フィクスチャ

- [ ] 緑 3 件: 実データ相当の複数行形（`reason` に `#751` を含む）/ 1 行形のインラインテーブル配列 /
      level のテーブル形 deny
- [ ] 赤 11 件: 受け入れ条件の変異 1〜7・8a・8b・9 と、`snotra-egui-runtime` だけを依存に持つ形（判定 C の誤爆検算）。
      **8a（節ごと欠落）を 8b（`warn` 降格）と別に置く**——Phase 1 の後は実リポジトリが判定 D を満たすため、
      `clippyMethodsDenied` が常に `true` を返す実装を**実データでは誰も捕まえられない**。赤フィクスチャが唯一の検知点である
      （節ごと欠落 → `false` は、現在のルート `Cargo.toml`（節が無い）に対する実測で確認済み）
- [ ] 赤ケースは**件数ではなく `file` の並び**で主張する（既存 `G-workspace-lints` の慣行——件数だけだと
      別クラスの退行で 1 件出た状態を満たしてしまう）
- [ ] `npm test` が緑

### Phase 4 — 文書の同期（**偽になる記述を同じ変更で直す**）

- [ ] `governance-check.mjs` の `G-workspace-lints` 冒頭「受容する残余」の第 1 項を訂正する。
      現文は「`[workspace.lints.clippy]` 等が降格されてもこの検査は鳴らない（clippy は `-D warnings` が
      コマンドライン側で昇格させており、workspace テーブルが担っていない）」で、**両方の節が偽になる**。
      「`disallowed_methods` は `G-clippy-disallowed` が見る。それ以外の clippy lint は依然として射程外」へ
- [ ] `src-tauri/clippy.toml` の「沈黙経路 0」段落を訂正する（deny 化した事実と、その deny 自身を
      `G-clippy-disallowed` が見張ること）。**経路 1〜3 は残るので消さない**
- [ ] 同ファイル末尾の「`[workspace.lints]` は…使えない」を鋭くする——否定しているのは**禁止集合（内容）**の
      移設であって、**レベル**は現に移したのだから、そう読める文にする
- [ ] `docs/build-commands.md` へ 1 文追加
- [ ] `docs/adr/ADR-clippy-disallowed-enforcement.md` を書く（採択 = 選択肢 2 + 判定 A/B/C、
      却下 = 選択肢 1「起動形の検査」・選択肢 3「記録に留める」、それぞれの理由）
- [ ] `npm run governance:check` が緑（**新規ファイルを含むので PR 前に必須**）。
      **コメントを触った作業項目ごとにその場で走らせる**——追記する散文が `G-references` /
      `G-heading-refs` / `G-stale-identifiers` の走査対象であり、パス様の文字列・見出し参照・識別子を
      書いた瞬間に自分で赤を作りうる。最後にまとめて走らせると、どの追記が原因か切り分けられなくなる

## findings クラス（異常系）

| クラス | 条件 | メッセージの型 |
|---|---|---|
| 母集団欠落 | `src-tauri/clippy.toml` が読めない | `… が読めない（G-clippy-disallowed 母集団の欠落）` |
| 母集団欠落 | `src-tauri/Cargo.toml` が読めない | 同上 |
| 母集団欠落 | ルート `Cargo.toml` が読めない | 同上。`G-workspace-lints` と重複して鳴るが、**沈黙させない側へ倒す**（黙って skip すれば、それ自体が新しい沈黙経路になる） |
| 判定 B | `disallowed-methods` の配列が無い | 「配列が無い」と明示（空配列と区別する） |
| 判定 B | カナリアの欠落 | **欠けたパスを名指しする**（直し方が読めるように） |
| 判定 C | `egui` 依存が無い | 「全パスが解決する前提が消えた」 |
| 判定 D | ルートの deny が無い/降格 | 「warn 既定へ戻り、禁止が黙って助言へ降格する」 |

## 不変条件

- **quote-aware なコメント除去を通す**（`tomlLine` を使わない）。破れば実データのエントリ行が `#751` で切れる
- **抽出は全域 match**（per-line 単発 `match` にしない）。破れば 1 行形で 1 件しか取れない
- **コメント行のエントリを数えない**。破れば「コメントアウトして空配列」が緑を通る（**最も起きやすい空洞化**）
- **判定 C は構文的位置で見る**（字面一致にしない）。破れば `snotra-egui-runtime` で常に緑

いずれも Phase 3 の赤フィクスチャが検知手段である（変異 4・9 と `snotra-egui-runtime` ケースが各々に対応）。

## 射程外（意図的・コメントへ明記する）

- `reason` 文言の変更 / `#[allow]` による迂回（lint に内在する性質）
- `clippy.toml` が cargo の fingerprint に入らないこと（`.rs` を触らず同じコマンドを打つとキャッシュ replay で
  exit 0。`clippy.toml` 冒頭コメントの経路 3 が正本）
- `disallowed_methods` 以外の clippy lint のレベル

## テスト方針と検証コマンド

```
npm test                                          # vitest（governance-check.test.mjs を含む）
npm run governance:check                          # カテゴリ F・新規ファイルを含むため必須
cargo clippy --workspace --all-targets -- -D warnings   # カテゴリ A（Cargo.toml を触るため）
cargo check --workspace                           # Cargo.toml 変更の健全性
```

実装後の再実測（`.claude/rules/safety-nets.md`「フォールトインジェクションで一度は実測する」）:
`src-tauri/src/*.rs` へ禁止メソッド呼び出しを 1 つ注入し、**`-D warnings` を付けない**
`cargo clippy -p snotra --all-targets` が exit≠0 になることを確認して、注入を戻す。

## 未確定（実装前に潰す）

- [x] `tomlLine` を再利用できるか — **不可**。実データのエントリ行が `reason` 内の `（#751）` で切れることを
      実測（`research.md` 制約 1）。`stripTomlComment` を新設する
- [x] 素朴な per-line 単発 match で足りるか — **不足**。1 行形で 1/2 件、コメントアウトされたエントリを
      「在る」と誤認する（実測）。quote-aware + 全域 match を採る
- [x] 選択肢 2 は `-D warnings` 依存を本当に消すか — **消す**。`-D warnings` 無しで exit 0 → exit 101 へ
      変わることを注入で実測（`research.md` 制約 3）
- [x] 他 3 crate を巻き込まないか — **巻き込まない**。`clippy.toml` が無く禁止集合が空。3 crate の clippy が
      exit 0・診断なしを実測
- [x] 判定 C が `snotra-egui-runtime` で誤爆しないか — **しない**。構文的位置で見る述語を実測（19 ケース全通過）
- [x] deny のキー綴りは `disallowed_methods`（アンダースコア）か — **そう**。この形で実測した。
      ハイフン形は述語が**赤へ倒す**（fail-closed）。`G-workspace-lints` の先例に倣い、直し方をコメントに書く
- [x] `SPEC.md` の同期が要るか — **不要**。先例 `ca8afae`（`git show --stat`）も触っていない
- [x] G-* の id を列挙する索引文書があるか — **無い**。ADR にも索引は無い（自動収集）
- [x] ADR を書くべきか — **書く**。選択肢 1 / 3 の却下は「否定の知識」であり、`.claude/rules/governance-docs.md`
      の条件に当たる。連番を振らない slug 形にする

## セルフレビュー

- リスク: 通常
- plan-review: 未実施（通常リスク）／自己レビューのみ
- エージェント数: 0
- 自己照合（`/start-issue` Step 5a の 5 点）:
  1. issue の全要件に作業項目が対応 — 判定 A/B/C は Phase 2、沈黙経路 0（採択された選択肢 2）は Phase 1
  2. 境界条件と検証 — 受け入れ条件の変異 9 件 + 誤爆検算 1 件を Phase 3 のフィクスチャが 1:1 でカバー
  3. 新しい状態・リソース・プロセス — 無し（純粋関数の静的読み取り）。異常系は findings クラス表
  4. より単純な既存パターンで置き換えられないか — `G-workspace-lints` と同型に寄せた。
     既存機構での代替は不成立（`G-references` が守るのは実在だけで、内容は無防備）
  5. 壊してはならない不変条件に検知手段 — 「不変条件」節の 4 件すべてに対応する赤フィクスチャを名指し
- 要対処: 3 件を計画へ反映済み — (a) 判定 D を「起動形の検査」ではなく「ルートの deny の実在」として
  設計（沈黙を移さない）、(b) 偽になる規範コメント 2 か所を Phase 4 の作業項目へ昇格、
  (c) コメントアウト経路（変異 9）を受け入れ条件へ追加（issue 本文の 6 経路には無い・実測で発見）
- 未検証: 無し（述語 19 ケース + 実 clippy 3 条件を計画段階で実測済み）

## 人間レビュー

- [x] 承認済み — 2026-08-06 / 問い: "`workspace/plan.md` へ注釈を追加していただくか、明示的にご承認ください。承認後に workspace をコミットし、`/implement` で実装へ進みます。" / 回答: "OK"

# review-1328 (sha ade83abf562e61a289ac202e5081743ec425c052)

状態: 完了（align exit 0・作業ツリーは変異後に巻き戻し済み）

## 所見

Critical / High: 0 件

### Medium
- M1 ⚠️ Step 3 :73 から落とした「`apply_migrations()` で移行」は、実コードでは真の手順である。`snotra-core/src/config/migrate.rs` の `migrate_instant_legacy_commands` が `InstantAction::Legacy { command }` を `Url` へ移している。plan.md は「CLAUDE.md :59 は逆に『apply_migrations() では救えない』と言う」ことを落とした根拠にしているが、:59 が言うのは「parse に失敗した場合は救えない」である。Legacy で受理した後に移行する段とは矛盾しない。結果として「受理した後に正規形へ移す」段は、スキルにも CLAUDE.md にも残っていない。必須の段かどうか（Legacy のまま保持してもよいか）は裁定が要る。
- M2 Step 4 の末尾「区分と各枝の保全の正本は `config/io.rs` の `Config::load_from_dir_reporting` の doc」は、`config/io.rs` の `//!` の割り振りと食い違う。`//!` は「区分は [`LoadOutcome`] が、各枝が何を保全するかは [`Config::load_from_dir_reporting`] の rustdoc が正本」と分けている。CLAUDE.md 側は `LoadOutcome` / `load_from_dir_reporting` の両方を指している。指す先の内容は存在する（fn の doc に 5 枝の箇条あり）ので壊れてはいない。ただし正本の主張がずれている。

### Low
- L1 ⚠️ CLAUDE.md に足した行「根拠（どの失敗から来た規則か）と…は `/persistence-check` が持つ」は 6 行の規則すべてに及ぶ書き方である。しかし TOML 移動の規則（`skip_serializing` + `take()`）と「最小 spike で往復バイト一致」の規則には、対応する失敗の型（典型パターン・アンチパターン）がスキルに無い。過大な主張である。
- L2 スキルはリポジトリルート基準で書かれているのに、`config/io.rs` だけが crate 内の短縮パスになっている。同じスキル内でも `snotra-core/src/window_data.rs` は完全パスで書かれており、不揃い。
- L3 典型パターン 2「（例: 絶対座標→モニター相対。`snotra-core/src/window_data.rs` の `//!`）」は述語が無く、例の出典なのかが読み取りにくい。「出典は〜」等にすると自然。内容は正しい（`//!` に「絶対→相対等のときは binfmt ヘッダ version バンプ必須」と書かれている）。
- L4 Step 5「Config をデシリアライズする新経路…」だけは、CLAUDE.md「Config のデシリアライズ経路」に同じ規則があるのに正本を指していない。他の 3 行は正準形で指しているので不揃い。問いの形なので受け入れ条件 3 の基準では残してよいが、参照を添える余地がある。

### 確認済み（問題なし）
- 識別子の集合差（旧版と新版のスキルを比較）で落ちたのは `Legacy {..}` と素の `apply_migrations()` だけ。`Legacy { .. }` は CLAUDE.md に残る。素の `apply_migrations()` はバッククォート付きで残る。足されたのは `//!`・`Config::load_from_dir_reporting`・`config/io.rs`・`snotra-core/src/window_data.rs`。
- インシデント番号: #343・#394・#461・#646 は残る。#338 は :79 に残る（:52 の誤用は意図的に削除）。
- 3 区分（NotFound / 内容破損 / 一時的失敗）は `load_from_dir_reporting` の doc に全部ある。「次の通常 save で昇格」・TOML 移動・Legacy 受理は CLAUDE.md にある。IndexCache の旧 7 項目要約は、CLAUDE.md の 9 項目の部分集合。
- 正準形 3 本（IndexCache・history・データ永続化の注意）はすべて CLAUDE.md の実見出しに着地する。governance:check の素の状態は green（17 件 passed）。
- CLAUDE.md の 6 行の字面は不変（diff は 1 行の追加と空行だけ）。

## 変異（受け入れ条件 7）

node_modules が無いので `node scripts/governance-check.mjs` で走らせた。

| 状態 | SKILL.md の hash-object | 結果 |
|---|---|---|
| 変異前 | 80a90285e25ce7bfde1ebff00736174368147a95（`git rev-parse ade83abf:…SKILL.md` と一致） | EXIT=0, 全検査 passed（検査 17 件） |
| 誤ラベル（:88） | cb0f60a477b300bc66ee350e567802b666fb3536 | EXIT=1 |
| 同じ誤ラベル行をフェンス内へ | — | EXIT=0, 全検査 passed（検査 17 件） |
| 巻き戻し（git checkout） | 80a90285e25ce7bfde1ebff00736174368147a95 | — |

赤の出力（全文）:
```
governance:check — 1 件の不整合:
  .claude/skills/persistence-check/SKILL.md:88  見出し参照が着地しない: `snotra-core/CLAUDE.md`「history.rs のキー正規化チェックリスト」（snotra-core/CLAUDE.md に該当する見出し・リード文が無い）
```
この文言を出すのは、製品の側では `scripts/governance/checks/G-heading-refs.mjs:62` だけである（他の一致は `*.test.mjs` の中）。モジュール解決の失敗や timeout ではない。

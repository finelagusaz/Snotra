# research — issue #489: health-check Check 7 が MEMORY.md の所在を書いておらず、委譲すると誤報する

## 1. issue の要約

`/retrospective` のサイクル末 health-check をサブエージェントへ委譲したところ、Check 7 が **「MEMORY.md が存在しない」** と報告した。実際には存在し、リンク先 5 件すべて解決する。**Check 7 は本来グリーンである。**

ユーザー承認済みの方針（案 A）: Check 7 に **所在**（リポジトリ外）と **委譲規約**（絶対パスをプロンプトに渡す）を明記し、「Glob で見つからないことは発見事項ではない」と書く。`/retrospective` Step 6 / Step 7 も同じ前提に立つため同時に補う。**Check 番号は変えない。**

## 2. 根本原因 — 「書き忘れ」ではなく「コンテキストの継承境界」

issue 本文は「Check 7 は所在を書いていない」と述べたが、調査するとより正確な記述がある。

> **メモリ領域の絶対パスは、メインエージェントの system prompt にだけ存在する。委譲した瞬間に落ちる。**

- メインエージェントの system prompt は `C:\Users\<user>\.claude\projects\<slug>\memory\` を明示的に与える
- サブエージェント（`Explore` 等）はこれを継承しない。ゆえにリポジトリ内を `Glob "MEMORY.md"` して「無い」と結論する（実測）

したがって「所在を書けば直る」だけでは不十分で、**「委譲するときは絶対パスをプロンプトに渡す」という規約**が要る。書くべきは定数ではなく手続きである。

### 2.1 実測（このセッションで取得）

| 測ったこと | 結果 |
|---|---|
| メインエージェントの `Glob(pattern="*.md", path="<memory dir>")` | **6 件返る**（`MEMORY.md` + メモリ 5 件） |
| メインエージェントの `Grep(path="<memory dir>/MEMORY.md")` | **5 件のリンク行を返す**。全リンク先が実在 |
| サブエージェントの `Glob "MEMORY.md"`（パス無し） | **0 件**（リポジトリ内を探した） |
| `git ls-files \| grep -c '^MEMORY.md$'` | **0**（リポジトリに無いのは事実） |

→ **`Glob` / `Grep` / `Read` は絶対パスでリポジトリ外へ届く。** ツールの到達性は問題ではない。届け先を知る者が委譲で失われることが問題。

## 3. もう一つの欠陥 — 二つの文書が互いを無効化している

| 文書 | 記述 | 含意 |
|---|---|---|
| `.claude/skills/retrospective/SKILL.md:107` | 「Check 1〜10 を**本スキルが直接実施する**（Read / Grep / Glob）」 | 委譲するな |
| `.claude/skills/health-check/SKILL.md:6-11`（`allowed-tools`） | `Bash(git *)` / `Read` / `Grep` / `Glob` / **`Agent`** | 委譲してよい |

`Agent` が許可ツールに入っているため、委譲は自然に起こる（本セッションで実際に起きた）。**規約が 2 文書に分かれ、片方が他方を無効化している。** どちらかに寄せねばならない。

`allowed-tools` から `Agent` を外す案もあるが、`/health-check` は user 起動時に 10 項目を並列化する価値があり、委譲そのものは禁じたくない。**禁じるのではなく、委譲するときの契約を書く**のが正しい。

## 4. 三つ目の欠陥 — 報告語彙が「判定不能」を持たない

Check 7 が今回出した報告は **「不在」**（Info）だった。正しくは **「検証不能」**（パスが与えられていない）である。

これは #482 で塞いだ失敗様式と同型である。

| | #482 の hook | #489 の Check 7 |
|---|---|---|
| 管轄外 | 対象ツール以外 → allow | — |
| **判定不能** | payload 破損 → **block** | パス未提供 → **「検証不能」と報告すべき** |
| 実際の挙動 | （修正済み） | **「不在」と報告した**＝存在しない欠陥をでっち上げた |

「検証できなかった」を「異常が無い」と読むのが fail-open、「異常がある」と読むのが false positive。Check 7 は後者に倒れた。報告語彙に**第三の値**が要る。

## 5. 関連コード（すべてドキュメント）

| ファイル | 役割 | 本 issue での扱い |
|---|---|---|
| `.claude/skills/health-check/SKILL.md` | Check 7 の定義・`allowed-tools`・出力フォーマット | **触る** |
| `.claude/skills/retrospective/SKILL.md` | Step 6（メモリ鮮度）・Step 7（health-check 実施） | **触る** |
| `AGENTS.md` | 「サイクル末 health-check の実行責任は `/retrospective` が負う」（L95）・環境制約の並列委譲 | **触る候補**（委譲はコンテキストを継承しない、という横断ルール） |
| `CLAUDE.md:105` | スキル表「10項目で検証」 | **触らない**（項目数は不変） |
| `docs/build-commands.md:116` | 「`/health-check` の Check 10 で検出する」 | **触らない**（番号は不変） |
| `docs/superpowers/**` | Check 5 / Check 9 への序数参照 | **触らない**（日付入り過去記録） |

`SPEC.md` は製品仕様であり、スキルはプロダクト挙動ではない → **更新不要**。

## 6. 既存パターン（再利用できるもの）

- **序数参照を保つ**: 案 A は Check 番号を変えない。ライブの序数参照が 4 箇所ある（`docs/build-commands.md:116` → Check 10、`health-check/SKILL.md:50` → Check 5、`retrospective/SKILL.md:107` → Check 1〜10、`CLAUDE.md:105` → 「10項目」）。案 B（Check 7 削除）はこの 4 箇所すべてを腐らせる——**今サイクルで `AGENTS.md` に書き足したばかりの失敗様式**
- **「判定不能」を第三の値として持つ**: `pre-bash.mjs` の `decide` が「管轄外 allow / 判定不能 block」を区別する構造をそのまま踏襲する
- **委譲時の情報受け渡し**: `/plan-review` Step 2b は「`workspace/plan.md` を読ませないことを明示する」と、**サブエージェントに何を渡し何を渡さないかを明文化**している。同じ書き方で「メモリ領域の絶対パスを渡す」と書ける

## 7. 技術的制約

- **絶対パスはマシン固有**（`C:\Users\Eoh\...`）。スキルはリポジトリにコミットされ共有されるため **ハードコード禁止**。書けるのは「所在の説明」と「どこから得るか」だけ
- **パスの形は harness 内部仕様**（`~/.claude/projects/<cwd をハイフン化したスラグ>/memory/`）。将来変わりうるため、**スラグの導出規則を正典として書かない**。「メインエージェントの system prompt に与えられる」が唯一安定した記述
- **`health-check` の `allowed-tools` は `Bash(git *)`** — `ls` / `cat` は使えない。`Read` / `Glob` / `Grep` で足りる（実測）
- **`health-check` は `disable-model-invocation: true`** — モデルからは起動できない。`/retrospective` Step 7 が定義を読んで直接実施する構造は変えない
- **health-check は「報告のみ・修正しない」** — この契約を壊さない

## 8. 未解決の疑問

1. **サブエージェントの system prompt にメモリ領域が含まれないことの一般性。** 本セッションの 1 体（`Explore`）で観測した。`general-purpose` 等でも同じかは未測。→ **計画では「含まれない前提で書く」**（含まれていたとしても、パスを渡す規約は無害）
2. **`Agent` を `allowed-tools` に残すか。** 残す（並列化の価値）。ただし委譲契約を書く。→ 計画で `/plan-review` に問う

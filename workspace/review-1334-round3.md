# #1334 3 巡目（sha 575c0b6146cd48d265dfdff39c3326e911373078）

align は exit 0。差分 3f702e77..575c0b61 は、pre-bash.mjs の 2 行（コメントだけ）と workspace/review-1334-round2.md の追加。

## N1 は解消したか

- 文面: 「一般則のうち、判定単位を構文的位置で定義する、という具体則からの意図的な逸脱」。逸脱の範囲は項目 6 の派生則 1 本に絞られている。「具体則」は development-principles.md の項目 6 自身の語（「派生する 2 つの具体則」）と一致する。→ 解消
- 正準形の着地: `governance:check` は 17 件すべて passed。見出しを「…階梯X」に壊す変異を当てると G-heading-refs が `.claude/hooks/pre-bash.mjs:19 見出し参照が着地しない` で赤になったので、この参照は照合に載ったまま。`git checkout` で戻し、hash-object が HEAD の blob（1e4f606d）と一致した。`git status` は clean（.delegate/ を除く）

## この差分が新しく持ち込んだもの

- 偽の記述、着地しない参照、意味の変化: なし
- ⚠️（ごく軽い・直さなくてよい）22 行目「例外を一般則の側へ書き戻さず」の「一般則」は、19 行目では節全体を指す語として使われている。一方、ADR-command-shape-norms-in-hook:38 の文脈では §6 の具体則を指している。どちらの読み方でも、文意（例外を原則文書へ書き戻さない）は変わらない

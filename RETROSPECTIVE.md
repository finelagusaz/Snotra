# Retrospective — verify-premises 原則の repo 移植 + skip-ci スコープ明文化 + #565 実装

（doc/skill のみ 3 ファイル・+6/-1 のメタサイクル。PR #570 内部前提の恒久化 / #571 skip-ci スコープ / #572 外部前提を start-issue へ = #565 完了）

## よかったこと

### 前提の裏取りを、このサイクル自身へ再帰的に当てた

サイクル全体が「issue の前提を鵜呑みにしない」の実践だった。①#565 は overkill か → 既存 3 箇所（Step 3 実在確認 / development-principles.md #409 / memory）の被覆を照合し「一般則は三重、外部次元だけ穴」と判定。②「移植は他環境の Claude に届く」か → 配送経路を表で検算し、memory はローカル・パス固定で届かず repo doc で初めて旅すると確認。③「.claude なら全部 skip 可」か → `vitest.config.ts` の `include` を実測し、hooks/githooks/scripts は CI が検査する＝skip 不可と判明。毎回一次資料（grep / gh api / vitest.config / ruleset）に当て、誤った分岐を未然に断った。

### 「やりすぎ」を核へ削り、削った理由を残した

#565 を受け入れ条件 6→1 に縮小。落とした 3 項目（version 一致・fixed/reverted 状態・research.md 新スキーマ）は #555 の失敗因でないと判定し、issue に「今回スコープ外・理由付き」で明記した。将来の読者が「抜け」と誤読して足し戻す経路を塞いだ。三層（内部=development-principles.md / 外部=start-issue / 選択肢空間=#409）が視界の割れで重複なく立った。

### 文書化した運用を即ドッグフーディングした

skip-ci の skip-safe 集合を build-commands.md に明文化した直後、その skills-only / doc-only PR（#571・#572）自身に skip-ci を貼り、運用を実地で検証した。close の確認も closingIssuesReferences（派生値）に留めず `gh issue list --search "closed:>=<mergedAt>"`（起きた事実）で #565 単独の close を裏取りした。

---

## 伸びしろ

### 汎用の教訓が memory 単独に留まり、版管理で旅していなかった

verify-issue-premises は汎用原則にもかかわらず、点在するローカル agent memory にしか無く、協働者・別マシンに届かず、次の retrospective 上書きで失われる経路にあった。「上書き前に教訓を抽出」の規律は機能していたが、抽出先が memory だと不十分——repo doc まで持って初めて届く。今回 development-principles.md へ移し memory はポインタへ縮小したが、「一般則の家は repo であって memory ではない」を早い段階で判断できると、恒久化までの遅延を減らせる。

### issue 番号の取り違えに 1 往復を要した

ユーザーは #568 と指したが内容は #565 だった。本文と description の矛盾に気づき即座に照合・確認できた（壊れた入力から推論しない、を実践）が、着手前に 1 往復のコストがかかった。既存の global CLAUDE.md rule #2 と verify-premises 原則が覆う範囲であり、新しい構造的教訓の追加は不要——照合の即応が効いた事例として記録に留める。

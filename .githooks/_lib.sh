# .githooks/ の共通部品。各 hook から source される（単体では実行しない）。
#
# 守るもの: main が進むこと。守り方: 操作ごとに git が呼ぶ hook で、
# 「実際に操作されるツリー」のブランチを見て判定する。git は hook を
# working tree のトップを cwd として起動するため、`git -C <other>` でも
# worktree でも、判定対象は常に実際の操作先になる。
#
# この層は best-effort。core.hooksPath はローカル設定なので外れうる。
# 外れたときの最終防衛線は GitHub ruleset（main への直接 push を拒否）。
# ゆえに「この層が生きているか」を検知する仕組みは、意図的に作らない。

PROTECTED_BRANCH=main

# 拒否して終了する。exit 1 が git に操作を中止させる。
die() {
  printf 'BLOCKED: %s\n' "$1" >&2
  printf '  feature ブランチ（feat/ fix/ chore/）を作成してから操作してください。\n' >&2
  printf '  判定: .githooks/（ローカル） / 最終防衛線: GitHub ruleset\n' >&2
  printf '  意図的な操作なら --no-verify で迂回できます（人間専用。エージェントは使用禁止）。\n' >&2
  exit 1
}

# 現在のブランチ名。detached HEAD では空文字列を返す（＝判定不能）。
current_branch() {
  git symbolic-ref --short -q HEAD || true
}

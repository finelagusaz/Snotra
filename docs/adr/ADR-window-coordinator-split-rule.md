# ADR-window-coordinator-split-rule: 段 1 の分割規則を 1 本に絞り、managed struct 化・listener 移設・z-order 集約を却下する

#749（段 1）で `egui_shell/window_coordinator.rs` を新設し、窓を駆動する 11 関数を集めた。移設そのものは機械的だったが、**実質は「線をどこに引くか」の判断**だった。ここに残すのは、その過程で却下した選択肢である。

## 文脈

issue は段 1 を「ほぼ移設・意味変化ほぼゼロ」と規定していた。しかし「窓の可視性・位置・サイズ・z-order・wake を 1 つの責務へ集める」を額面どおり取ると、どこまで引き込むかが一意に決まらない。

計画に対する MECE レビューで、**線が 5 つの異なる原理で引かれており、衝突したときにどちらが勝つかが書かれていない**と指摘された（責務の種類 / 唯一の消費者 / 実行スレッド / 段 3 の前提 / フレーム内の読み点）。実例として、`read_metrics` と `max_results` はどちらも「唯一の消費者が移設対象の中にある」形なのに、計画は片方だけを運んでいた。判別規則が無いまま線を引くと、後から「これはどちらの責務か」に答えられなくなる。

## 決定

1. **規則 R を 1 本だけ置く（例外ゼロ）**: 移設する関数がその中でしか使わないヘルパーは一緒に運ぶ。複数のモジュールから消費されるものは残す。適用結果は `read_metrics` と `max_results` を運び、`read_visual`（消費者 2）と `window_width`（view 内 2 用途）を残す
2. **listener の登録（`register_*` の 4 関数）は全て `mod.rs` に残す**
3. **z-order は集約しない。** 新モジュールの `//!` と `src-tauri/CLAUDE.md` に不在を明記する
4. **main 窓のサイズ適用は 2 か所に分かれたままにする**（show 経路の bar_height collapse は coordinator、毎フレームの動的高さは `view.rs`）
5. results のサイズ memo は `ResultsWindow` へ、判定式は `layout::size_delta_exceeds` へ。**呼び出し点は 2 つ（results 窓と main 窓）だが、状態は共有しない**
6. `reset_size_guard` の呼び出しは view の reset-on-show に残す

## 検討した代替案と却下理由

### 1. managed struct `WindowCoordinator` を新設し、`ResultsWindow` + waker + guard を集約する

`drive_results_window` が `&mut self` を要求する唯一の理由は view-local なデルタガード 2 フィールドである。その行き先は 3 つしかなかった——(a) `ResultsWindow` の内部へ、(b) view に残して `&mut` で渡す、(c) 新しい managed struct へ集約する。

(c) を却下した。`main.rs` の `app.manage` 構成と、`commands/window.rs` のポーリングスレッドから `set_topmost` へ至る到達経路を書き換えることになる。issue は段 1 を「ほぼ移設」と規定しており、**#666（段 3）はモジュール割りを一切指定していない**ため、その割りを先取りする根拠が無い。(b) は段 1 の目的（view から窓の状態を引き上げる）を達成しないので却下し、(a) を採った。

### 2. 規則を「config を読むヘルパーは `mod.rs` に集める」にする

規則 R の対立候補。却下した。`window_width` が view に残る理由を説明できず、**例外が 1 つ生じる。** 規則 R は同じ 4 件を例外ゼロで説明する。**例外を持つ規則は、次に線を引く人が「今回も例外では」と考える余地を残す。**

### 3. `register_hide_listener` だけを coordinator へ移す

「hide の合流点だから hide の実体と同居させる」という筋は通る。却下した。その規則を当てると、`show_egui_main` と `wake_main` を呼ぶ `register_initial_hotkey_failure_listener` が `mod.rs` に残る理由を説明できない。**「登録は全て `mod.rs`」なら例外がゼロになる。** setup 配線の一覧性を `main.rs` の 1 画面に残す設計（`EguiShellHandles` の doc）とも揃う。

### 4. z-order を coordinator へ引き取る

issue の責務表は z-order を挙げている。却下した。main の最前面切り替えは `commands/window.rs` が `set_always_on_top` を直接叩き、results は `ResultsWindow::set_topmost` が持つ（tao の差分適用が results を消すため層が違う・#646 PR2）。**呼び出し元はどちらも設定サイドカー監視のポーリングスレッド**であり、coordinator を通す形にすると依存の向きが増える。「ほぼ移設」の範囲を超える。

**代わりに `//!` から z-order を落とした。** 当初の `//!` 草稿は z-order を宣言していたが、実体は 1 行も入っていない——`AGENTS.md`「検証の作法（全タスク共通）」が「全称表現は前提条件とセットで書く。書けないなら書かない」と定めるとおり、**書けないなら書かない**側へ倒すのが正しい。

### 5. main 窓のサイズ適用を引き取る

却下した。ADR-results-presentation-two-stage 却下 1 の第 3 理由（main の高さは `show_egui_main` の bar_height collapse と `main_window_height` の**意図的な 2 導出**）を段 1 が巻き戻すことになる。前者は位置クランプが展開時の高さで効くのを防ぐ折り畳み、後者は status / toast 行の増減への追従であり、目的が違う。

**当初この判断を `//!` へ「main のサイズ適用は `view.rs` にある」と書き、誤りだった。** show 経路の折り畳みは `show_egui_main` の中にあり、その関数はこの段で coordinator へ移っている。**自らの説明が自らの主張を反証する形**になっていた（コードレビューで検出）。正しい記述は「2 か所に分かれたまま」である。

### 6. `reset_size_guard` を `show_egui_main` へ移す

memo の所有を `ResultsWindow` へ寄せた以上、reset も窓側の経路（show）へ寄せる方が対称に見える。却下した。**`show_egui_main` は egui のイベントループとは別のスレッドから走りうる**（setup・single-instance コールバック・hotkey listener・alt 解放待ちの spawn を含む 5 経路を実測）。現在この reset はイベントループスレッド上で、同一フレームの `drive_results_window` より前に起きる。移すとフレーム進行中に割り込みうる——`Mutex` ゆえデータ競合は無く最悪でも「余分な `set_size` が 1 回」で済むが、**スレッド同一性という現行の前提を変えることになり「意味変化ほぼゼロ」を外れる。**

### 7. `set_size` の戻り値を `bool` にする

`show()` / `hide()` と揃えたくなる。却下した。あちらが `bool` を返すのは**呼び出し側が trace を 1 回だけ出すため**であり、`set_size` に対応する trace は無い。使われない `bool` を返すと「遷移を見て何かする経路がある」と読ませる。

### 8. 判定式を切り出さず、手書きのまま `ResultsWindow` へ移す

却下した。`ResultsWindow` は `tauri::Window` を持つためユニットテストできない（`.claude/rules/src-tauri.md`）。**判定式だけを純粋核へ出せば `0.5` の許容境界にテストを置ける**——この移設で得られる唯一の自動カバレッジである。さらに `/dry-check` で、**同じ式（同じ閾値）が main 窓側にも手書きされていた**ことが分かった。呼び出し点は最初から 2 つあった。

ただし**状態は共有しない**。main の memo（`last_set_*`）は view に残る——却下 5 のとおり main の高さは意図的な 2 導出であり、その状態を窓の所有型へ寄せない。**式を共有することと、状態を共有することは別である。**

## 帰結

- 規則 R は例外を持たない。将来「これはどちらの責務か」を問われたら R に当てる。**例外を作るなら、その時点で R は規則ではなくなる**ことを意識して判断する
- **z-order と main のサイズは集約されていない。** `//!` と `src-tauri/CLAUDE.md` に明記した。「1 つの責務へ集めた」という全称表現を、前提条件なしで書かないため
- 段 3（#666）の前提は動いていない。段 1 は `view.rs` から driver と 2 フィールドを抜いただけで、managed state の構成には触れていない
- 却下 5 の教訓は一般則である: **責務の所在を書くときは、根拠に挙げた関数が今どこにあるかを確かめる。** 移設を伴う変更では、説明文のほうが先に古くなる

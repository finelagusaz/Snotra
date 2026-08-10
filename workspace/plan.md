# 起動計器の未確認な検知器と残置を潰す（issue #1009）

**ゴール**: #1000 が作った検知器のうち**当てていない変異 7 件に実際に当て**、落ちる/落ちないを実測する。
落ちないものは**なぜ落ちないかを doc へ書く**（受け入れが明示的にそれを認めている）。あわせて未測定の
2 値を測って記録し、残置 3 件（L-6 / M-5 / L-4）と判断 1 件（M-2）を閉じる。

**方針**: 変異は**複製・使い捨てビルドにだけ当て、稼働中のガードを弱めない**
（`.claude/rules/safety-nets.md`「フォールトインジェクションでは、稼働中のガードを弱めない——複製に変異を当てる」）。
**変異点・実行条件・予測を測る前に固定する**——同ルールの「注入が正しい強さであることは別である」に当たる。

**成果物の性質**: この issue の主たる成果は**測定と、その結果を運ぶ doc** である。コード変更は
L-6 の手当てと doc 追記に限られ、ハーネスへの検知器追加は**素通りが実測されたときだけ**行う（下の決定規則）。
**検知器 2 本（L-6 の非負性・(e) の `event` と `ok`/`reason` の整合）はブレストで既定として承認済み**
（下の「決定」）——**実測で素通りが確認できなければ足さない**という順序は変わらない。

**この計画は棚卸を兼ねる**（2026-08-10 のブレストで判明）。計器の着地（`1133fa6`）以降、
`PERFORMANCE.md` の「起動の端から端まで」の表は**一度も更新されていない**——`git log 1133fa6..HEAD --
PERFORMANCE.md` は #1023 と #1010 を返すが、両者が触ったのはメモリ計測の節であり、起動の表ではない。
その間に #1023（`perf(core)!`）が `indexer.rs` を 2117 行書き換えている。**Phase 0 のベースラインは
「緑であることの確認」ではなく「#1023 後の再測定」である**（下の Phase 0）。

## 受け入れ条件（issue の 4 群に対応）

1. 変異 (a)(c)(d)(e)(i)(j)(l) の全件について、**当てた結果**（赤 / 素通り / 実行不能とその理由）が記録されている
2. `GetProcessTimes` と `SystemTime::now()` の順序が決まり、`pre_main` が負にならないことと誤差の向き・
   大きさが**実測値として**記録されている。`SystemTime::now()` の分解能が **Rust 側で**測られている
3. L-6 が閉じている（**β = doc + ハーネスの非負性検査**。下の「決定」）。M-5 / L-4 の doc が追記されている
4. M-2 が「直す」「受容する」のどちらかに倒れ、根拠とともに記録されている
5. **`PERFORMANCE.md` の起動の表が #1023 後の値へ更新されている**（棚卸ぶん・下の「決定」）

**「記録されている」の宛先は会話ではなくファイルである**（`PERFORMANCE.md` / `startup.rs` の `//!` /
`bench-startup.ps1` の doc）。

## 変異マトリクス（実行前に固定する）

**当てる順序は下の表のとおり**（安い順・製品側の変異は 1 つずつ当てる——まとめると片方の赤がもう片方を隠す）。

| # | 変異（注入点） | 実行条件 | 予測される検知器 | 予測 |
|---|---|---|---|---|
| (l) | `Test-StartupPayload` の**複製**へ **`post_main_ms == Σ phase_ms`** を足す | 再ビルド不要（保存した実ペイロードへ当てる） | その等式自身 | **赤**（切り捨てゆえ実起動で必ず食い違う） |
| (d) | `Timeline::sum_phase_ns` の返り値を `* 2` | 実 config・release | ハーネス検査 3（恒等式・飽和側）＋ `cargo test`（`sum_of_phase_ns_equals_the_last_mark`） | **赤** |
| (i) | `finish()` の `post_main` を `anchor.elapsed().saturating_sub(t.last)` へ | 実 config・release | ハーネス検査 3 | **赤** |
| (c-A) | `main.rs` の `set_branch` を `Branch { first_run: true, cache_hit: false, include_path_env: false }` リテラルへ固定 | **実 config**（実際は first_run=false / cache_hit=true / include_path_env=true） | 検査 2 の逆向き（`null` であるべき区間に値がある）が `index_load` と `path_merge` を名指す | **赤**。ただし **`cache_hit` の偽りは素通り**（説明者に使われていない） |
| (c-B) | 同上を `include_path_env: true` へ固定 | **検証用プロファイル**（実際は false） | 検査 2 の順向き（説明されない `null`: `path_merge`） | **赤** |
| (e) | `finish()` の `event` を無条件 `"startup:ready"` へ | **占有された `Alt+Q`**・検証用プロファイル | （無い） | **素通り**——検査はキーの存在だけを見て `ok` / `reason` の**値**を一度も見ない |
| (j) | `main.rs` の `send_initial_hotkey_registration()` 成功直後に `startup::finish(Ok(()))` を足す（arm 側の 2 度目は `FINISHED` の CAS が捨てる） | 実 config・release | 検査 4 は**上限のみ**・恒等式は保たれる | **素通り**——終端値が実際より小さくなる方向は原理的に見えない |
| (a) | `finish()` 冒頭の `FINISHED` の CAS を削除 | — | — | **実行不能**。二重終端を起こす唯一の経路（platform 初期化失敗）は ADR が永久に測らないと決めた経路と同一 |

**(a) の付随実測**: (j) の変異と (a) の変異を**同時に**当てると、注入点を足さずに二重終端を作れる
（送信直後の `finish` と arm の `finish` が両方通る）。**製品経路ではないが、ハーネスが重複を畳むこと**
（`Wait-SnotraTraceCondition` の `Select-Object -Last 1`）**の実測にはなる**——(a) の結論（実行不能）は
変えないが、「仮に二重終端が起きてもハーネスは沈黙する」ことを測って doc へ書く。

### 予測が外れたときの決定規則（実装前に決めておく）

- **赤を予測して素通りした** → その変異を**捕まえる検知器が無い**ということである。素通りの事実と機序を
  doc へ書き、検知器の追加は下の「人間レビュー」で承認された範囲でだけ行う
- **素通りを予測して赤になった** → 変異が**本来の回帰より強い**疑いがある（`.claude/rules/safety-nets.md`）。
  赤の理由が予測した機序と同じかを確かめ、違うなら変異を弱めて当て直す
- **どちらの場合も、変異を当てたビルドはコミットしない**

## フェーズと作業項目

### Phase 0 — 準備（測定の土台）

- [x] `cargo build --release -p snotra` で素の release を作り、`npm run bench:startup`（実 config・
      **7 標本**）を走らせる。**これは緑の確認ではなく #1023 後の再測定である**——2026-08-09 の表と
      同じ標本数で取り、`index_load` / `path_merge` が #1023（`indexer.rs` 2117 行の書き換え・
      背景再スキャン撤去）でどう動いたかを読む。ここが赤なら変異の赤は読めない
      → **2026-08-10 実測（7 runs passed）**: index_load 1/1/26・tauri_init 6/7/8・
      windows_create 29/33/38・path_merge 12/13/32・hotkey_register 18/20/21・post_main 71/82/143・
      pre_main 7/8/19（min/p50/max）。run 1 が cold で全区間の max を持つ
- [x] **同日 A/B を取る（計画外・実測中に必要と判明）** — 2026-08-09 との差を #1023 の効果と読むには
      **旧バイナリを同日に測る**ことが要る（`PERFORMANCE.md`「warm frame は日をまたいで比較しない」が
      同じ罠を名指ししている）。#1023 の親 `4e0616b` を worktree（`C:/workspace/snotra-pre1023`）へ
      出してビルドし、同条件で 7 標本を取る。**測る前に実 config を退避する**——旧バイナリは背景
      再スキャンを持ち、形式昇格も担っていた（`%APPDATA%/Snotra` の 5 ファイルを scratchpad へ複製済み）
- [x] 測った値を `PERFORMANCE.md`「起動の端から端まで」へ**日付つきの節として足す**（既存の
      2026-08-09 の表は消さず残す——**履歴は無負債**であり、#1023 を挟んだ A/B は計器の `//!` が
      自称する存在理由〔「上流の改修の前後で同じ器を当てられること」〕の一発目の実証になる）
      → **A-B-A で実測**（現行 → 旧 `4e0616b` → 現行・各 7 標本）。**post_main は A1 82 / B 79 /
      A2 71 ms** で、**同一バイナリの日内変動 11 ms が B との差を飲み込む**——#1023 の起動時間への
      効果はこの計器では測れない。**常駐だけは本物**（旧 44.9〜49.4 MB に対し現行 39.0〜40.6・
      日内差 1.5 MB を超える）。**2026-08-09 との差（index_load 8→1・tauri_init 3→7）は日差であり
      改修の効果ではない**——旧バイナリを今日測っても 2 / 6 ms になる。`PERFORMANCE.md` に新節と、
      2026-08-09 の表への注記を置いた
- [x] 占有スクリプトを `scripts/` へ置く（`RegisterHotKey(IntPtr.Zero, 1, 0x4001, 0x51)` =
      `MOD_NOREPEAT|MOD_ALT` + `VK_Q`。**`platform/hotkey.rs` の `register_prepared` と同じ引数**）。
      握ったまま待機し、`Ctrl+C` で解放する形にする。**`.NOTES` には撤去条件ではなく「なぜ常設か」を
      書く**——(e) の検知器（下の「決定」）を再検算する唯一の手段であり、`AGENTS.md` の撤去条件の義務が
      懸かる「一時的な足場」ではない
      → **`-DurationSeconds` を足した（計画外）**。計画は Ctrl+C 解放だけを想定していたが、
      **実行者がエージェントのとき対話端末が無い**。秒数指定と Ctrl+C の両対応にし、「プロセスが
      終われば OS が解放する」ことを `.NOTES` へ書いた。3 秒で実行し、握れて解放されることを確認済み
- [x] 占有下でベースラインを取る（**変異なし**・`-UseVerificationProfile`）。
      **期待は `startup:failed` / `reason=hotkey-registration` でハーネスが赤**——これは失敗報告そのものが
      効いていることの実測であり、(e) の対照である
      → **予測どおり赤**（2 run とも `startup:failed reason=hotkey-registration` /
      `reached_phase=hotkey_register`・exit 1）。**issue の (e) 前半——「実機で登録を失敗させ、
      `startup:failed` が出ることとハーネスが理由つきで落ちることを測る」——はこれで満たした。**
      残るのは (e) の変異（`event` を無条件 `startup:ready` へ）が素通りするかである

### Phase 1 — 変異を当てる（マトリクスの順）

- [x] (l) を `Test-StartupPayload` の複製へ当て、保存した実ペイロード（7 標本）で測る
      → **式の形を実測で確定させた（計画の転記が誤っていた）**。issue の `total == pre_main +
      Σ phase_ms` は**当てられない**——`total` という項目が出力に無い。私が計画へ転記した
      `post_main_ms == pre_main_ms + Σ phase_ms` も誤りで、**`pre_main` は `post_main` の外側**
      （プロセス作成 → main 突入）ゆえ足すと二重になる（実測: `pre+Σ` = 73〜75 に対し
      `post_main` = 70）。当てられる形は **`post_main_ms == Σ phase_ms`** で、**7/7 で不成立**
      （差 3〜4 ms・9 区間ぶんの切り捨ての積み上がり）＝**予測どおり赤**。
      **`git log -S` で確認: この式が過去に在った痕跡は無い**（`bench-startup.ps1` の履歴は計器の
      着地コミット 1 本のみ）——issue の「再導入する」は事実ではなく、新規に当てる予測だった
- [x] (d) を当てて `npm run bench:startup`（実 config）を走らせ、結果を記録する
      → **赤（予測どおり・ただし予測より広い）**。`cargo test -p snotra` が **4 本**落ちた
      （`sum_of_phase_ns_equals_the_last_mark` / `post_main_is_taken_independently_of_the_partial_sum` /
      `rounding_happens_only_at_the_display_boundary` / `unmarked_tail_closes_the_sum_when_the_last_phase_never_ran`）。
      **予測は 1 本だった**——`sum_phase_ns` は 4 本が共有する土台なので、変異は
      「総和の検算を壊す」より広く効く。ハーネス側も検査 3 で赤
      （`post_main_ns=75668200 != 151336400`・ちょうど 2 倍）。復元を `git diff` で確認済み
- [x] (i) を当てて同上
      → **素通り（予測は「赤」・外れた）**。`cargo test` 241 全緑・ハーネス `passed`。
      機序: 同語反復化すると `post_main == sum_phase` ゆえ `unmarked_tail = post_main - sum_phase = 0`
      になり、検査 3 の恒等式は **`X == X + 0` で必ず真**。`bench-startup.ps1` の doc が自ら
      「同語反復化は素通りする（実測）」と書いていたとおりで、**#1000 で実測済みの (h) と同型**だった
      ——issue が (i) を「未確認」に数えたのは、基準点の差し替え（anchor 以外のマーク）と
      同語反復化を別物として数えたためだが、**`sum_phase` を基準にすると両者は同じ形に落ちる**
- [x] **(i) で予定外の検知器が出た** — `post_main` を anchor から作るのをやめると `anchor` が
      未使用になり、**`-D warnings` の clippy が最初に落ちる**（`unused variable: anchor`）。
      **`//!` にも `research.md` の検知器一覧にも無い経路**であり、コンパイラが同語反復化の
      第一検知器である。ただし `_anchor` にすれば通るので、**弱い**（意図的な変異は素通りできる）
- [x] **(i) は `hotkey_register` を 0 ms にする** — 同語反復化した `post_main` をそのまま
      `mark(HotkeyRegister, …)` へ渡すため、差分が構造的に 0 になる。**`PERFORMANCE.md`
      「計器が計器の欠陥を暴いた」が記録した「7 回とも 0 ms」と同じ署名**が出るが、
      **検査は 0 を咎めない**（人間の目にしか映らない）
- [x] (c-A) を当てて同上。**`cache_hit` の偽りが素通りすることを明示的に確かめる**
      → **赤（予測どおり）**。検査 2 の逆向きが 2 run × 2 区間を名指した:
      「null であるべき区間に値がある: index_load = 1631800 ns」「同 path_merge = 12668800 ns」。
      **`cache_hit=False` の偽りは一言も咎められていない**（予測どおり素通り——出力の
      `cache_hit=False first_run=True path_env=False` に現れているが、どの判定にも使われない）
- [x] (c-B) を当てて `-UseVerificationProfile` で走らせる
      → **赤（予測どおり）**。検査 2 の順向きが「説明されない null: path_merge」を 2 run とも出した。
      **(c-A) と併せて、検査 2 の双方向が両向きとも実証された**
- [x] (e) を当てて占有下・`-UseVerificationProfile` で走らせる（Phase 0 の対照と比べる）
      → **完全に素通り（予測どおり）**。ホットキー登録が実際に失敗しているのに `起動計器 passed`。
      **ペイロードは矛盾を抱えたまま通った**——`ok=False` / `reason=hotkey-registration` は
      正直に載り、`event` だけが `startup:ready` を騙っている。検査 1〜4 はキーの**存在**しか
      見ず、**値を一度も読んでいなかった**。Phase 0 の対照（同条件・変異なし）は赤だったので、
      差は `event` の 1 行だけである
- [x] **(e) の検知器を足し、同じ変異ビルドで赤くなることを測った** — `Test-StartupPayload` へ
      検査 5（`event` と `ok` / `reason` の整合）を追加。同一条件で **passed → 赤**へ反転:
      「event が ok と食い違う: event=startup:ready / ok=False / reason=hotkey-registration
      （期待 startup:failed）」。**偽陽性も両経路で測った**——正常起動（`ok=true`）は緑、
      実際に失敗した起動（`ok=false`・変異なし）は「起動が失敗した」1 件のみで検査 5 の行は出ない
- [x] **検知器の追加が契約検査の母集団を広げた（副産物）** — 以前は `startup:failed` の run が
      `continue` で検査を飛ばしており、**失敗した起動のペイロードは一度も契約検査を受けて
      いなかった**。検査 5 は騙られた run（`event=startup:ready`）にも届く必要があるため
      呼び出し構造を直し、**失敗経路も検査 1〜5 を通るようにした**
- [x] (j) を当てて実 config で走らせる
      → **素通り（予測どおり）**。`post_main` が 71〜82 → **54〜58 ms** に化けたのに `passed`。
      検査 4 は**上限だけ**を縛るので、内側の申告が小さくなる方向は原理的に見えない。
      `hotkey_register` はまた **0.00 ms**（(i) と同じ署名）——**残り時間が「起動していない」
      ことになる**方向の変異であり、実害の向きとしては (i) より重い
- [x] (a)+(j) の同時変異で二重終端を作り、ハーネスの挙動を測る
      → **二重終端は作れた**（生 trace に `startup:` が **2 行**）。**ハーネスは `passed`**——
      `Wait-SnotraTraceCondition` の `Select-Object -Last 1` が重複を畳み、最後の 1 行しか見ない。
      **(a) の結論を訂正する**: 「実行不能」なのは**製品経路の再現**であって、**検知器の監査では
      なかった**。(j) との同時変異で「一度きり性を外したときハーネスが検知するか」は測れ、
      答えは**検知しない**である
- [x] 各件の結果（赤/素通り・出た文言・機序）を下の「実測結果」表へ書き込む
- [x] `git status` で作業ツリーに変異が残っていないことを確認する（`git diff -- src-tauri/` が空）

### Phase 2 — 測って記録する 2 件

- [x] `SystemTime::now()` の分解能を **Rust 側で**測る。tight loop で相異なる値の最小差を取り、
      標本数と最小/中央値を記録する。**常設のテストにはしない**（環境依存の値を assert すると間欠的に赤くなる）。
      **測定コードは scratchpad に置いてコミットしない**（下の「決定」）
      → **min / 中央とも 100 ns**（200 標本 / ループ 397 回 / max 2400 ns）。**PowerShell で測った
      0.0015 ms（1500 ns）と一桁違う**——issue が「代理では測らない」と指定したのは正しかった。
      issue が保険を掛けた「最悪粒度 15.6 ms」は実測で 5 桁小さい
- [x] `GetProcessTimes` の creation 取得と `SystemTime::now()` の**順序を決める**
      → **issue の前提が誤っていた（実装を読んで判明）**。issue は「順序を決めよ」と言うが、
      **順序は誤差の向きを決める**——`pre_main = now - created` で `created` は過去の固定値ゆえ、
      `now` が先なら差は**小さく**出る（現状は `now` が先＝過小評価側）。ただし入れ替えで動く額は
      `SystemTime::now()` 1 回の所要（中央 0 ns / max 200 ns・1000 標本）で、**`pre_main` の
      粒度（ms）に届かない**。現状の順序を維持し、測定値つきで `pre_main_elapsed` の doc へ書いた
- [x] `pre_main` の誤差の向き・大きさを測る
      → **計画の記述が 2 つの誤差を混同していた**。`begin()` の順序（anchor → `pre_main_elapsed()`）が
      乗せるのは**正方向**の誤差で中央 0 ns / max 100 ns、`pre_main_elapsed` 内部の順序が乗せるのは
      **負方向**で中央 0 ns / max 200 ns。**別の誤差である**。両方とも ms に届かない。
      `begin()` の doc の「順序が額を決めるわけではない」も実測に合わせて訂正した
- [x] `pre_main` が負にならないことを確かめる（`checked_sub` が `None` を返す形＝出力は `null`）
      → **実機 7 標本で 7.5〜14.8 ms・負値 0 件・`null` 0 件**
- [x] 測定値を doc へ記録する → **`PERFORMANCE.md` ではなく `startup.rs` の
      `pre_main_elapsed` / `begin` の doc へ置いた**（計画は `PERFORMANCE.md` を指していた）。
      これは**運用点の測定値ではなく実装の性質**（時計の分解能・順序の誤差）であり、
      `PERFORMANCE.md` は運用点の記録が正本である。co-location の側に寄せた

### Phase 3 — 残置 3 件

- [x] **L-6（β で確定）**: `index_load_unattributed_ms` の非負性は**現在の呼び出し形では成り立つ**。
      2 前提（外側が内側を包む・両者が切り捨て）と、**破れたときに負値が panic せず出力に現れる**ことを
      `to_json` の当該ブロックへ書いた。**#1023 で前提が実際に動いた実例も添えた**
- [x] **L-6（β の機構側）**: `Test-StartupPayload` へ検査 6（`index_load_unattributed_ms >= 0`）を追加。
      **変異で落ちることを測った**——保存した実ペイロードの当該値を `-1` に書き換えると赤（再ビルド不要）、
      素のペイロードでは破れ 0 件（偽陽性なし）
- [x] **M-5**: 「起動時 1 回」制約を `PlatformCommand::RegisterInitialHotkey` の variant 宣言と
      `send_initial_hotkey_registration` の両方へ置いた。**帰結だけを書いて正本（`startup.rs` の `//!`）を
      指す形**。**issue の記述を鵜呑みにせず実在を確認した**——代替として案内する variant は
      `UpdateHotkey` ではなく **`SetHotkey`** である（grep で確認）
- [x] **L-4**: `unmarked_tail_ns` の 1 行を `//!`「区間は網羅列挙する」へ置き、正本（`to_json` の
      当該ブロック）を指した

### Phase 4 — M-2 の判断

- [x] 二重起動経路を実機で踏み、`bench-startup.ps1` から見えるかを確かめる
      → **2 つ目は exit code 0 / 95 ms / stderr 0 行**（終端も trace も 1 行出さない）。
      **1 つ目は終端 1 行のまま**（`show_egui_main` は計器を触らない）。ハーネスは各 run の前に
      既存プロセスを殺すので、**この経路を踏むことは原理的に無い**
- [x] 「直す」「受容する」のどちらかに倒し、根拠とともに `//!` へ 1 行置く
      → **「受容する」**。実装は変えず、`//!` に「二重起動は終端を出さない（受容・実測）」節を置いた。
      **手で踏んだときも取り違えようがない**——2 つ目の stderr が空だからである

### Phase 5 — 記録の反映と検証

- [x] Phase 1 の実測結果を `bench-startup.ps1` の `Test-StartupPayload` の doc へ反映する
      （**素通りが実測された検査の弱さは、その検査の隣に書く**。既に検査 3 はその形で書かれている）
- [x] **(e) の検知器を足す** — 検査 5（`event` と `ok` / `reason` の整合）を追加した
- [x] **(e) の検知器が捕まえることを測る** — 変異ビルド・占有下で **passed → 赤**へ反転を実測。
      偽陽性は正常起動（`ok=true`）と実失敗の起動（`ok=false`）の両経路で 0 件
- [x] **(j) は doc へ倒す**（検知器を足さない）— 検査 4 の doc へ「この検査が見ないもの」を書いた。
      下限が無いので終端を手前で打ち切る変異は原理的に素通りすること、下限を置かない理由（trace の
      到着遅れと区別できない）を、`events.rs` の `event_names_are_pairwise_distinct` と同じ形で
      **検査 4 の隣**に置いた
- [x] `startup.rs` の `//!`「受容する残余」を実測後の姿へ更新した（**測ったものと測っていないものの
      区別**を保った——実機観測済みは `HotkeyRegistration` だけ・一度きり性の検知手段が無いことを明記・
      二重起動の節を新設・既存ハッチが代理である理由を追記）
- [x] `cargo fmt`（OK）/ `cargo clippy -p snotra --all-targets -- -D warnings`（緑）/
      `cargo test -p snotra`（241 passed・`--lib` は付けない）/
      `cargo doc --workspace --no-deps --document-private-items`（exit 0・警告は `snotra-core` の既存分のみ）
- [x] `npm run governance:check` — 全検査 passed（19 件 / 見出し参照 184 件）
- [x] `npm run bench:startup`（素の release・7 標本）が緑（**検査 1〜6 すべて有効な状態で**）

## code-reviewer の指摘と対応（ラウンド 1・2026-08-10）

**High 2 件はどちらも私の誤りだった。** 全件 fix-forward で当て、修正差分を同じ道具で再実行した。

- [x] **High 1: 測っていない量の測定値を載せていた** — `pre_main_elapsed` の順序を入れ替えたとき
      動くのは **`GetProcessTimes` の所要**であって `SystemTime::now()` の所要ではない。
      「中央 0 ns / max 200 ns」は後者の値で、**帰属が誤っていた**。**対象そのものを測り直した**:
      `GetProcessTimes` 1 回は **min 100 / 中央 200 / max 5400 ns**（1000 標本）。
      あわせて「5 桁小さい」（下端で不成立）→「最悪の 5400 ns でも 3 桁下」、「中央 0 ns」→
      「`Instant` の分解能未満」、7 標本の出所が 2 つ混在していた点も直した
- [x] **High 2: 実在しない害を理由に挙げ、実在する害を書いていなかった** — `RegisterInitialHotkey`
      の 2 度目は**終端を 2 行出さない**（`FINISHED` が捨てる——doc が自分で否定していた）。
      実際の害は `hotkey::register` が同じ `HOTKEY_ID` で失敗し（先に `UnregisterHotKey` を
      呼ばない・コードで確認）、`INITIAL_HOTKEY_FAILED` が飛んで**偽の登録失敗通知が出る**こと。
      **無効な理由を書くと「FINISHED が面倒を見るなら制約は要らない」と読まれて外される**
- [x] **Medium 1**: 「検査は 3 つある」が 6 項目を並べていた（`ae3335d` 時点で既に腐っており、
      **私が 2 度編集しながら見逃した**）。数を書かず各項を正本とする形へ。宙ぶらりんの
      `workspace/plan.md` 参照も落とした
- [x] **Medium 2**: 検査 5 に「見ないもの」が無い非対称（検査 3・4 は持つ）。`event` と `ok` は
      同じ `outcome` から導かれるので**`outcome` 自体の誤りは素通りする**——射程を隣に書いた
- [x] **Medium 3**: `occupy-hotkey.ps1` の「唯一の手段」が同ファイル 2 段落上で反証されていた。
      検査 5 の発火に要るのは `ok=false` の payload であって実登録失敗ではない。
      **「変異なしで `ok=false` になる対照はこれでしか取れない」**へ狭めた
- [x] **Medium 4**: `PERFORMANCE.md` の常駐。指標が `WorkingSet64`（private WS ではない）で、
      測定窓が終端 +1.5s・**旧側は背景再スキャン走行中**であることを明記。差は p50 同士の
      6〜7 MB ではなく**範囲の最接近 4.3 MB** で述べる形へ
- [x] **Low 1**: 「ハーネスがこの経路を踏むことは無い」の全称否定 → ハーネスが区別材料
      （「本体が終了（exit=0）」「trace 行 0」）を出す事実を書く形へ（`SnotraSmoke.psm1:650` で確認）
- [x] **Low 2**: 二重終端の機序。`Select-Object -Last 1` ではなく**1 行見つけた時点で待機を抜ける**
- [x] **Low 3**: `a > b ⇒ floor(a) ≥ floor(b)` → `a ≥ b ⇒ …`（外側 = 内側でも非負性が要る）
- [x] **Low 4**: **私の差分が新しく作った重複**（`Test-StartupPayload` の呼び出し 2 か所・引数一致）。
      検査を分岐の手前へ出して 1 か所に畳んだ
- [x] **Low 6**: 「Win32 の排他は modifiers と vk の組に働く」は未測定の仕様主張 → 測ったこと
      （同じ引数なら失敗する）だけに狭めた
- [x] **Low 7**: `try` を登録の直後から開く形へ（生成/破棄のペアを構造で守る）
- **Low 5（受容）**: `.ps1` の見出し参照は G-heading-refs の母集団外。**既知の残余として受容する**
      ——`.ps1` を母集団へ入れるのはガバナンス機構の変更であり、この issue の射程を超える

**修正差分の再実行**（`AGENTS.md`「レビュー指摘へ修正を当てた」行）: 検査ロジック 3 ケース・
実機 2 経路（正常/実失敗）・`cargo fmt` / `clippy` / `test`（241 passed）・`governance:check`（19 件）・
`cargo doc`（`snotra` crate の警告 0）・`bench:startup` 7 標本、いずれも修正前と同じ結論。

## code-reviewer の指摘と対応（ラウンド 2・修正差分の検算）

**ラウンド 1 の修正が 1 件で逆側へ振れ、周辺に 4 件の弱点を作っていた**——`AGENTS.md`
「修正は指摘箇所へ注意が集中し、周辺に新しい誤りを生む」が実際に起きた形である。

- [x] **Medium A（振れすぎ）**: 「ハーネスは 1 行見つけた時点で抜けるので 2 行目は読まれない」は
      **実測した状況で偽になりうる**。`Wait-SnotraTraceCondition` は**スナップショット単位**で拾い、
      その中の最後の 1 行を返す（`SnotraSmoke.psm1:630-640` で確認）。(a)+(j) が作る 2 行は数 ms 差で
      ポーリング刻みは 100 ms ゆえ**同一スナップショットに入る公算が高く、読まれるのは 2 行目**。
      ラウンド 1 の版（`Select-Object -Last 1` が畳む）とラウンド 1 の修正版（1 行目で抜ける）は
      **どちらも半分しか真でなかった**。両方を覆う不変条件「**返るのは常に 1 行だけ**」へ差し替えた
- [x] **Low B**: `begin()` の「`Instant` の分解能未満（max 100 ns）」が自己矛盾（非零の実測値がある）。
      かつ**未測定の量（`Instant` の分解能）を新しく持ち込んでいた**。「中央が 0・max 100 ns」へ
- [x] **Low C**: `SystemTime::now()` の分解能の段落が、High 1 の修正で**孤立した**（何の主張も
      支えなくなった）。「`pre_main` の値そのものの粒度の下限」として接続し直した
- [x] **Low D**: `Wait-SnotraTraceCondition` の**表示文言 2 本**へ結合していた（`.ps1` は
      G-heading-refs の母集団外ゆえ、文言が変われば沈黙で腐る）。性質だけを書く形へ
- [x] **Low E**: `occupy-hotkey.ps1` の括弧が「偽陽性を測る対照にならない」と書いていたが、
      **検査 5 から見てハッチ経由と実失敗の payload は同一**（どちらも `ok=false` /
      `reason=hotkey-registration` / `event=startup:failed`）。「測っている対象が実失敗であること」
      を担保する、へ狭めた
- [x] **Low F**: `Get-SnotraPrivateWorkingSetMB` は `WorkingSet64`（プロセス全体）を返しており
      **名前が嘘だった**。`Get-SnotraWorkingSetMB` へ改名し、呼び出し 1 か所を更新・旧名の由来を doc へ
- [x] **Low G**: `try` 移動の副作用で `if ($DurationSeconds -gt 0)` が 2 回出ていた。1 つに畳んだ

**ラウンド 2 の検算結果（レビュー側の実測）**: Low 4 の畳み込みは失敗終端 3 種すべてで挙動不変
（`$terminal -eq $null` は `$data` の手前で `continue` するので新しい呼び出しに到達しない・
コンソール出力の順序も不変——契約検査は `Write-Host` を持たない）。Low 7 の `try` 移動は
`finally` の到達性を実際に改善（`throw` 経路は `try` の外＝未登録ゆえ解放不要）。High 1 の帰属は
両 doc とも正しい量を指す。

**ラウンド 2 の修正後の再実行**: `cargo fmt` OK / `clippy -D warnings` 緑 / `cargo test` 241 passed /
`cargo doc`（`snotra` crate の警告 0）/ `governance:check` 19 件 / 実機 2 経路（正常＝緑・
実失敗＝「起動が失敗した」のみ）/ `bench:startup` 7 標本 passed。

## 実測結果（Phase 1 で埋める）

**8 件すべて当てた。予測が外れたのは (i) と (a) の 2 件である。**

| # | 予測 | 実測 | 出た文言・機序 |
|---|---|---|---|
| (l) | 赤 | **赤** | `post_main_ms == Σ phase_ms` が 7/7 で不成立（差 3〜4 ms＝9 区間ぶんの切り捨て）。**式の形は計画の転記が誤っており、実ペイロードで測って確定させた**（`total` は出力に無い・`pre_main` は `post_main` の外側） |
| (d) | 赤 | **赤（予測より広い）** | `cargo test` が **4 本**落ちた（予測は 1 本）。`sum_phase_ns` は 4 本が共有する土台。ハーネスも検査 3 で赤（`75668200 != 151336400`・ちょうど 2 倍） |
| (i) | 赤 | **素通り（外れ）** | 同語反復化で `unmarked_tail = 0` になり、検査 3 は `X == X + 0` で必ず真。**#1000 で実測済みの (h) と同型**だった。**予定外の検知器**: `anchor` が未使用になり `-D warnings` の clippy が最初に落ちる（`_anchor` にすれば通るので弱い）。`hotkey_register` が 0.00 ms になる |
| (c-A) | 赤（`cache_hit` は素通り） | **赤・`cache_hit` は素通り** | 検査 2 の逆向きが 2 区間を名指し: 「null であるべき区間に値がある: index_load = 1631800 ns」「同 path_merge」。`cache_hit=False` の偽りは一言も咎められない |
| (c-B) | 赤 | **赤** | 検査 2 の順向き: 「説明されない null: path_merge」。(c-A) と併せて**双方向が両向きとも実証された** |
| (e) | 素通り | **素通り → 検知器を足して赤** | 素通り時: 登録が実際に失敗しているのに `passed`。`ok=False` / `reason=hotkey-registration` が正直に載ったまま `event` だけが騙る。**検査 5 追加後**: 「event が ok と食い違う: event=startup:ready / ok=False / reason=hotkey-registration（期待 startup:failed）」 |
| (j) | 素通り | **素通り** | `post_main` が 71〜82 → 54〜58 ms に化けても `passed`。検査 4 は上限だけを縛る。`hotkey_register` が 0.00 ms |
| (a) | 実行不能 | **測れた（外れ）** | (j) との同時変異で二重終端を作れた（生 trace に `startup:` が 2 行）。**ハーネスは `passed`**——`Select-Object -Last 1` が畳む。**実行不能なのは製品経路の再現であって、検知器の監査ではなかった** |

### 変異が明かした横断的な事実

- **`hotkey_register` の 0.00 ms が 2 つの異なる変異の共通署名である**（(i) と (j)）。
  `PERFORMANCE.md`「計器が計器の欠陥を暴いた」が記録した配置ミスも同じ署名だった。
  **人間の目には映るが、検査は誰も 0 を咎めない**——3 つの異なる欠陥に共通する signal である
- **検査 2 の説明者は 3 つと書かれているが、実際に判定へ使われるのは 2 つである**
  （`first_run` / `include_path_env`。`cache_hit` は出力するだけ・(c-A) で実測）

## 不変条件と異常系

- **稼働中のガードを弱めない**——変異は複製・使い捨てビルドにだけ当てる。Phase 1 の最後に `git diff` が
  空であることを確認する
- **占有スクリプトは `Alt+Q` を握る**。解放し忘れると以降の起動が全部失敗するので、Phase 0 の対照を取った
  直後に必ず解放し、素の bench が緑に戻ることを確かめる
- **doc へ写しを作らない**（`.claude/rules/governance-docs.md`「書く約束」）。M-5 / L-4 / L-6 はいずれも
  「正本を指す 1 行」であり、`//!` の全文複製ではない
- **全称表現を書かない**——「`index_load_unattributed_ms` は負にならない」は前提つきの主張である。前提を
  書かずに断定しない（`AGENTS.md`「検証の作法」）

## テスト方針と検証コマンド

| 対象 | コマンド |
|---|---|
| Rust 単体 | `cargo test -p snotra`（**`--lib` を付けない**——`[lib]` を持たない） |
| 静的 | `cargo fmt --all -- --check` / `cargo clippy -p snotra -- -D warnings` |
| 計器の契約 | `npm run bench:startup`（実 config）・`-UseVerificationProfile`（CI 相当） |
| ガバナンス | `npm run governance:check` |
| doc リンク | `cargo doc --workspace --no-deps --document-private-items`（`//!` を触るため・`.claude/rules/comments.md`） |

**CI 側の実測（runner での再測定）は PR が在って初めて行える**ので、PR 本文のチェックリストへ送る
（`.claude/rules/safety-nets.md`）。

## SPEC.md・関連文書の更新要否

- **`SPEC.md`: 不要**——計器の追加ではなく、既存計器の検知能力の実測と doc である。起動フロー・状態遷移を変えない
- `PERFORMANCE.md`: **要**（Phase 2 の測定値）
- `src-tauri/CLAUDE.md`: **不要**（`startup.rs` の行は既に「正本は `//!`」と書いており、数や射程を写していない）

## 未確定（実装前に潰す）

**3 件とも 2026-08-10 のブレストで裁いた**（下の「決定」が結論と根拠を持つ）。この節に残る項目は無い。

## 決定（2026-08-10 のブレスト・未確定 3 件を裁いた）

- **L-6 は β** — doc + `Test-StartupPayload` へ `index_load_unattributed_ms >= 0`。**根拠は調査中に出た**:
  #1023 で `total_started` の起点が「`load_or_scan_with_stats` の入口」から「`load_or_scan_with_stats_in`
  の入口」へ動いた（`Config::config_dir()` が内側の外へ出た）。**包みが広がる向きだったので非負性は保たれた**が、
  「機構で守られていない前提」が**たった 1 コミットで実際に動く**ことの実例である。doc は前提が破れた
  瞬間には働かない。(γ)（`saturating_sub`）は「測れなかった（`null`）」と `0` を混ぜるので却下
- **(e) は検知器・(j) は doc** — 素通りの構造が違う。(e) は検査 2 が `ok` / `reason` の**値**を見ないだけで
  1 行で塞げ、Phase 0 の占有対照がそのまま効果の実測になる。(j) は検査 4 の**下限が意図的に不在**（trace の
  到着遅れ）であり、塞ぐには別形の発明が要る——**#1008 の既定「射程を doc に書く」は (j) にこそ当たる**
- **足場は占有スクリプトだけコミット** — (e) の検知器を足すと決めた以上、占有スクリプトは**その検知器の
  検算手段**であり常設側に寄る（`.NOTES` には撤去条件ではなく「なぜ常設か」を書く）。分解能の測定コードは
  一度きりゆえ scratchpad
- **数値記録は Phase 0 を再測定へ格上げ** — 引き金の不在が乖離の実体だった（#1023 も #1010 も
  `PERFORMANCE.md` を触りながら起動の表だけ触っていない）。表は**日付つきスナップショットとして足す**
  （既存を消さない・履歴は無負債）。**条件別チェックへ規範行を足す案は採らない**——`ADR-retire-norm-review`
  が規範の判別力ゼロを実測している
- **計器のコード圧縮は採らない（別 issue 送り）** — 乖離が住んでいるのは**主張の面**（`PERFORMANCE.md` の
  数値・`research.md` の古い一行）であってコード量ではない。この計器は隣接する 2117 行の書き換えを
  **マーク 0 変更**で生き延びており、本体 ~460 行は `null` と `0` を混ぜない意味論と失敗の分類を担う。
  圧縮をなお望むなら #1009 の付随ではなく独立の issue とする

## 人間レビュー

- [x] 承認済み — 2026-08-10 / 問い: "計画のゲートは「承認済み」に倒しました。**実装へ進むかは、あなたの
      お言葉を待ちますわ。**" / 回答: "承認"
- 併せて、上の「決定」5 件はそれぞれ個別の問い（`AskUserQuestion`）で承認を得ている——L-6 は
  "β: doc + ハーネス 1 行の検査（推奨）"、(e)/(j) は "(e) は検知器・(j) は doc（推奨）"、足場は
  "占有はコミット・分解能は scratchpad（推奨）"、数値記録は "Phase 0 を再測定へ格上げ（推奨）"

## セルフレビュー

- リスク: 通常＋**セーフティネットの変更を含む**（`Test-StartupPayload` へ検査 2 本——L-6 の非負性と
  (e) の `event`／`ok`・`reason` の整合）。**ルート `CLAUDE.md` 最重要ルール 2 の合意は 2026-08-10 の
  ブレストで得ている**（上の「決定」）。手順は `.claude/rules/safety-nets.md`——**足した検査は変異で
  落ちることまで測る**
- plan-review: 未実施（`/plan-review`「リスク判定」の高リスク条件〔永続形式・並行性・網羅性が要件・
  ガバナンス文書の移動圧縮分割〕のいずれにも当たらない。セーフティネットの変更は safety-nets.md 側の
  手順が担う）
- エージェント数: 0
- 要対処: 自己レビュー 5 点の照合結果 — (1) issue の 12 項目すべてに作業項目が対応する（4 群 × 各項目を
  Phase 0〜4 へ割り当て済み）(2) 境界条件は変異マトリクスが「実行条件」として持つ（実 config / 検証用
  プロファイル / 占有下の 3 通り）(3) 新しいリソースは占有スクリプトの `RegisterHotKey` だけで、解放経路を
  Phase 0 と「不変条件」に置いた (4) より単純な既存パターン: (e) に `SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE`
  を使う案は**代理ゆえ却下**（研究メモ）(5) 壊してはならない不変条件「稼働中のガードを弱めない」の検知手段は
  Phase 1 末尾の `git diff` 空確認
- 未検証: 変異の予測そのもの（それを測るのがこの計画である）。**予測が外れたときの規則を先に書いてある**

# research: issue #743 — フォルダ展開中の ← が階層を上げない（と観測された）

## issue の要約

通常検索で実行ファイルを選び `←`（親を展開してフォルダ展開モードへ）、**もう一度 `←`** を押したとき、
「階層が上がらず選択だけが『実行ファイルの格納されたフォルダ』の行へ動く」と 2026-07-26 の実機で観測された。
受け入れ条件は (1) `←` で親フォルダ内容へ置換（SPEC §6.1・§6.2 のルート終端維持）、(2) 展開後の選択位置を確定し
SPEC §6.1 へ明文化、(3) `search_state.rs` の純粋核テストで固定。

## 関連コード（実在と行番号を grep で確認済み・起点 836f9d8）

| 位置 | 役割 |
|---|---|
| `src-tauri/src/egui_shell/view.rs:180` | `left = ctx.input(key_pressed(ArrowLeft))`（**非破壊読み**。`events.retain` で消費するのは ↑↓ だけ） |
| `src-tauri/src/egui_shell/view.rs:331-332` | `on_nav_keys(...)` の唯一の呼び出し。ガード無しで毎フレーム呼ばれる |
| `src-tauri/src/egui_shell/launcher_controller.rs:1057-1087` | `←` の処置。`match view_kind()` で Tool / Folder / Results の 3 分岐 |
| 同 `1060-1068` | Folder 分岐: `parent_dir()` → `navigate_folder(parent)` → `folder_cache=None` → `spawn_folder_load` |
| 同 `1069-1085` | Results 分岐: `compute_parent_dir(sel.path)` → `enter_folder(parent)` → **`record_folder_expansion(parent)`** |
| `search_state.rs:236-258` | `enter_folder` / `navigate_folder`（両者とも `selected = 0`・`folder_gen += 1`） |
| `search_state.rs:268-270` | `parent_dir()` = `compute_parent_dir(frame.current_dir)` |
| `search_state.rs:390-420` | `compute_parent_dir`（ドライブルート `X:\` / UNC `\\server\share` で `None`） |
| `search_state.rs:282-284` | `accept_folder_result(token)` = `tool.is_none() && token == folder_gen && folder.is_some()` |
| `launcher_controller.rs:914-937` | FolderMsg の drain。受理されたものだけ `folder_cache` へ入れ `run_search()` |
| 同 `680-689` | `run_search` の Folder 腕。**cache 未着の間は前フレームの行を保持し `set_results` を呼ばない** |
| `snotra-core/src/folder.rs:141-152` | `sort_entries_unlimited`。並びは `is_folder` 降順 → **展開回数降順** → 小文字名昇順 → path 昇順 |

## 判明した事実

### 1. issue の仮説（Results 分岐へ落ちて同じ親を再展開）は成立しない

`←` の分岐は `view_kind()` の `match` であり、`view_kind()` が `Folder` を返す条件は `folder.is_some()`。
`folder` を `None` に戻す経路は `on_escape`（`SearchState::on_escape`）と `reset()`（show ごと）の 2 つだけで、
どちらも 2 打鍵の合間には走らない（`run_search` / `set_folder_filter` / drain は `folder` を触らない）。
よって 2 回目の `←` は必ず Folder 分岐へ入り、`current_dir` は祖父へ書き換わる。

### 2. 観測された選択位置は「祖父へ正しく上がった」ときにしか到達しない（実測）

1 回目の `←`（Results 分岐）だけが `record_folder_expansion(親)` を呼ぶ。フォルダ列挙の並びは
展開回数降順を名前昇順より優先するため、祖父を列挙すると**その親フォルダが辞書順を無視して row 0 に来る**。
`navigate_folder` は `selected = 0` 固定なので、選択は「直前まで居たフォルダ」の行に載る。

一時テスト（`snotra-core/tests/tmp_probe_743.rs`・計測後に削除）での実測:

```
祖父 G の中身 = Aaa/ Mmm/ Zzz/ readme.txt、Zzz だけを record_folder_expansion 済み
PROBE rows = ["Zzz", "Aaa", "Mmm", "readme.txt"]   → row 0 = Zzz（＝直前まで居たフォルダ）
```

親フォルダの listing に「親フォルダ自身」は現れないため、**階層が上がっていなければこの選択位置には到達できない**。
残る食い違いは「リストが同じ内容のままに見えた」という観測側だけである。

### 3. 「上がったのに上がって見えない」を説明しうる 2 つの構造

- **現在フォルダの表示が UI に無い**（`search_state.rs:260-265` の doc: folder 中の hint 文脈提示は §6 で任意扱い・
  #532 SU3 M2 Task 3 で見送り）。入力欄は folder filter を表示するだけで、パス表示は存在しない
- **cache 未着の窓では前フレームの行が残る**（`launcher_controller.rs:688`）。`navigate_folder` は同期で
  `selected = 0` にするため、**列挙結果が届かない限り「行は前のまま・選択だけ動く」**という症状の形そのものになる。
  この窓が恒久化する条件は (a) worker の `app.try_state::<AppState>()` が `None` で早期 return（送信が 1 件も起きない）、
  (b) drain のフレームが起きない（`spawn_folder_load` の `request_repaint()` は**メイン窓の ctx**）。
  列挙失敗は `FolderMsg::Failed` → エラー行表示になるので別の見え方になる

### 4. 「直前のフォルダが選ばれる」は現状では偶然であり、2 段目以降は成り立たない

`←` による上昇（Folder 分岐）は `record_folder_expansion` を**呼ばない**（§4.6・Finding #1 の意図的な非対称）。
よって祖父からさらに `←` を押すと、曾祖父の listing で祖父の展開回数は 0 のままとなり、row 0 は別の行になる。
「直前まで居たフォルダが選ばれる」は Results からの 1 段目でだけ成立する副作用である。

## 技術的制約

- フォルダ列挙は worker スレッドで行い、`FolderMsg` + token（`folder_gen`）で staleness 判定する非同期経路
- ランタイムはイベント駆動。worker からの `request_repaint()` が無いとフレームが回らない（`spawn_folder_load` の doc）
- Win32 API の新規使用は無い（本 issue の範囲では入力・ウィンドウ系 API に触れない）
- 観測は GUI 上でしか起きないため、`←` 2 打鍵の実機観測には人間の打鍵が要る（memory: Win32 入力トレース協働スモークと同型）

## 裁定（2026-07-28・ユーザー選択）

- **根本原因の確定手段**: 「協働トレーススモーク」— `←` ハンドラと drain に一時トレースを入れた debug ビルドを用意し、
  `SNOTRA_TRACE` + stderr 捕捉でユーザーに 2 打鍵してもらい照合する
- **受け入れ条件 2（展開後の選択位置）**: 「**row 0 固定を as-built で明文化**」— 実装は変えない。
  上の事実 4（1 段目でだけ直前フォルダが選ばれる偶然）も SPEC に限界として書く

## 5. 協働トレーススモークの結果（2026-07-28 実機・症状は再現せず）

一時トレース（`probe743:left` / `:drain` / `:applied`）を入れた debug ビルド（起点 836f9d8 相当）を
`SNOTRA_TRACE=1` + stderr 捕捉で起動し、ユーザーが実機で打鍵。トレースは撤去済み。

`C:\Toolbox\ghost-launcher\ghost-launcher.exe` を選んだ 1 回目の試行:

| seq | 事象 | 内容 |
|---|---|---|
| 17 | `←` #1 | `view_kind=Results`, rows=200, selected=2 |
| 19 | 適用 | `current_dir=C:\Toolbox\ghost-launcher`, rows=**1**, selected=0 |
| 20 | `←` #2（10 秒後） | `view_kind=`**`Folder`**, `current_dir=C:\Toolbox\ghost-launcher`, `parent_dir=C:\Toolbox` |
| 22 | 適用 | `current_dir=`**`C:\Toolbox`**, rows=**133**, head=`["ghost-launcher","Snotra","SSP"]`, selected=0 |

- `←` は計 7 回。`probe743:drain` は **7 件すべて `accepted:true`**、`probe743:applied` も 7 件——
  棄却されたトークンは 0 件で、行は毎回差し替わった（事実 3 の「cache 未着の窓」は発生していない）
- `view_kind=Folder` での `←` は 2 回あり、どちらも親へ正しく上がった
- **ユーザーの画面確認（スクリーンショット）でも `C:\Toolbox` の内容が表示されていた**。選択行は row 0 の
  `ghost-launcher`（＝直前まで居たフォルダ・事実 2 のとおり）

**結論: 受け入れ条件 1 は現行コードが既に満たしており、issue の症状は不具合ではない。**
2026-07-26 の観測は、(a) 選択行が「直前まで居たフォルダ」の名前になる（事実 2）ことと、
(b) 現在フォルダの表示が UI に無い（事実 3）ことが重なった誤読と判断する。

## 未解決の疑問

- なし（根本原因は確定）。残るのはスコープの決定（受け入れ条件 2・3 の実施範囲と、
  誤読の原因である「現在フォルダの可視化」を本 issue に含めるか）——ユーザー裁定へ回す

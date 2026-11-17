# 計画: #1449 フォントのバイト列を、書き込みを拒む共有モードでメモリマップする

## 目的と受け入れ条件

フォントのバイト列をヒープへ写さず、ファイルを写像して積む。写像はファイルを書き込みを拒む共有モード（`FILE_SHARE_READ | FILE_SHARE_DELETE`）で開いて行い、開けない・写像できない・写像してはならない場所のファイルは、いまの方法（読んで写す）へ戻る（2026-11-17 の決定・issue のコメント。共有モードは試作の実測の後、削除も許す側へ決め直した——下の「決定の経緯」）。

- [受け入れ 1] アイドル・表示・検索の各段の PrivComm（PrivateBytes）が、写したバイト列のぶん下がる（HackGen 構成・Segoe UI + jp_font 構成の両方で測る）
- [受け入れ 2] `npm run bench:startup` の windows_create が悪化しない
- [受け入れ 3] 文字の表示が変わらない（`npm run check:colors` と目視・カテゴリ D）
- [受け入れ 4] 写像している間、そのフォントファイルを書き込みのために開くことは拒まれる。削除・名前の変更・同じ名前での置き換えは通り、そのあとも写像の中身は変わらない（外からの書き換えでクラッシュする経路が無い）
- [受け入れ 5] 同じファイルは 1 回だけ写像する（別名の family・user_font と jp_font が同じファイル）

## 設計（`src-tauri/src/egui_shell/font_stack.rs`）

### 写像の単位: `FontFile`

```rust
/// 写像したフォントファイル。開いたハンドルを写像と同じだけ持つ。
struct MappedFont {
    map: memmap2::Mmap,
    _file: std::fs::File, // FILE_SHARE_READ | FILE_SHARE_DELETE で開いたハンドル。写像の内容を変えさせない根拠
}
impl AsRef<[u8]> for MappedFont { fn as_ref(&self) -> &[u8] { &self.map } }
```

- `fn load_font_file(path: &Path) -> Option<(Blob, Loaded)>`（`egui::epaint::text::Blob` = `Arc<dyn AsRef<[u8]> + Send + Sync>`。`Loaded { Mapped, Copied }` は「写像の経路を通ったか」をテストが確かめるための値）
  1. `OpenOptions::new().read(true).share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)` で開く（値は `windows` クレートの定数を使う）。開けなければ（共有違反を含む）3 へ
  2. **開いたハンドルから**写像してよいかを判定する（パスの文字列では判定しない）: `GetFinalPathNameByHandleW(.., VOLUME_NAME_GUID)` で `\\?\Volume{GUID}\` を取り、`GetDriveTypeW` が固定ドライブ（値 3。`DRIVE_FIXED` は未有効の feature にあるのでローカル定数）であること、かつ属性にクラウドのプレースホルダ・オフライン（`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` / `RECALL_ON_OPEN` / `OFFLINE`）が無いこと。判定は純粋関数 `maps_on(drive_type, attributes) -> bool` に切り出す。どの失敗も「写像しない」側へ倒す。長さ 0 も写像しない。満たせば `unsafe { Mmap::map(&file) }` → `Arc::new(MappedFont { map, _file })`（`Mapped`）
  3. 1 か 2 で写像できなければ `std::fs::read(path)` → `Arc::new(Vec<u8>)`（`Copied`・いまの方法）。それも失敗なら `None`
- `MappedFont` のフィールドは `map` を先に置く（drop は宣言順なので、写像を解いてからハンドルを閉じる）
- `unsafe` の根拠（`SAFETY` コメント）: 写像の背後のファイルは、`FILE_SHARE_WRITE` を含まない共有モードで開いたハンドル（`_file`）が生きている間、他のハンドルから書き込みのために開けない（OS が拒む）。開けたことは、書き込みのアクセスを持つ既存のハンドルが無かったことを意味する。削除と名前の変更は許すが、それはディレクトリの項目を外すだけで、開いているハンドルと写像が指す中身は閉じるまで残る（2026-11-17 に実測）。`_file` は写像と同じ値の中にあり、写像より先に閉じない。ページの読み込みに失敗しうる場所（ネットワーク・プレースホルダ）は写像しない。memmap2 の内部のハンドル複製は根拠に使わない（実装の詳細）
- `Mmap` と `File` は `Send + Sync` なので、`unsafe impl` は書かない

### キャッシュをパスで持つ

- 写像のキャッシュは型（`FontFiles`）にし、製品は `static FONT_FILES: OnceLock<FontFiles>`（never-clear）、テストは自分でインスタンスを作る。キーは `std::fs::canonicalize` した正規化パス（user_font は fontdb が組んだ `C:\WINDOWS\Fonts\...`、jp_font は `C:/Windows/Fonts/...` と綴りが違うため）。錠は開く・判定・写像の間も持つ（2 スレッドが同時に引いても 1 回だけ写像する）。user_font と jp_font が同じファイルでも、別名の family でも、写像は 1 つ
- 錠の順序は `USER_FONTS` → `FONT_FILES`、`JP_FONT` → `FONT_FILES` の 2 本だけで、逆向きは無い（`/race-check` で確かめる）
- never-clear の役目は「同じファイルを 1 回だけ写像する」ことへ移る（`Blob` は `Arc` なので memory safety の要請ではなくなる）。代償: font_family を切り替えた後も、前のフォントファイルは終了まで書き込みを拒まれたまま
- `USER_FONTS` は family 名 → `(Blob, u32)` のキャッシュのまま（fontdb の照会を省くため）。値は `FONT_FILES` の `Arc` を clone したもの
- `JP_FONT: Mutex<Option<Blob>>`（見つからなかったことはキャッシュしない・今と同じ）
- `Box::leak` は消える（`Blob` は `Arc` なので `'static` が要らない）

### user_font の解決

- `resolve_font_family`: `db.query` で face を選ぶところまでは今と同じ。`db.face_source(id)` が `(Source::File(path), index)` なら `load_font_file(&path)`。`Source::Binary(arc)` / `Source::SharedFile(_, arc)` は、その `Arc` をそのまま `Blob` として使う（型が同じなので写さない）。fontdb の `make_shared_face_data` は使わない（共有モードを絞らずに開くため）

### 積む側

- `font_definitions(jp: Option<Blob>, user: Option<(Blob, u32)>)`: `FontData::from_blob(blob, index)` で積む（jp は index 0）。`tweak` と family の並び（index 0 の不変条件）は今と同じ
- `font_covers_cjk(bytes: &[u8], ..)` は `blob.as_ref().as_ref()` を渡す（シグネチャは変えない）

## 変更するファイルとシンボル

1. `src-tauri/src/egui_shell/font_stack.rs`: `MappedFont`・`Loaded`・`FontFiles` / `FONT_FILES`・`load_font_file`・`maps_on`（いずれも新規）・`JP_FONT`・`ResolvedFont`・`USER_FONTS`・`resolve_font_family`・`jp_font_bytes`・`resolve_stack`・`font_definitions`・テスト。doc は `//!`・`JP_FONT`・`font_covers_cjk`・`font_definitions`・`ResolvedFont`・`USER_FONTS`・`resolve_font_family`・`jp_font_bytes`・`resolve_stack` のコメント（「13.26 MiB が常駐」）・`prefetch`（「漏らす回数」）を、写像と `Arc` の持ち方に合わせて直す
2. `src-tauri/Cargo.toml`: `memmap2 = "0.9"`（`Cargo.lock` の版 0.9.12 と同じ）
3. `PERFORMANCE.md`: 「常駐メモリの帰属」の後に採用の記録（前後の PrivComm）。「egui 期のメモリ実測」の注記（141 行）の「機構（`from_static` / …）だけが生きている」を、写像へ替えた後も真になる形へ直す
   あわせて、現在形で持ち方を述べている記述（199〜201 行の never-clear・225〜226 行の leak・2728 行の「フォントのバイト列は 1:1 で private commit に載る」）に、#1449 以降は当てはまらないことを節頭の注記の形で指す
4. `SPEC.md`（710 行の font_family の項）: 「起動中は、使っているフォントファイルをその場で書き換えられない（写像のため。削除・名前の変更・置き換えはできる）」を 1 文足す
5. `src-tauri/CLAUDE.md` の `font_stack.rs` の説明行は「フォント解決と `set_fonts` 登録」のまま（変えない）

## 不変条件と異常系

- 写像の内容は、写像が生きている間、通常のファイル API では変わらない（共有モード）
- 写像できないとき（共有違反・長さ 0・ローカルの固定ドライブでない・写像の失敗）は写す方法へ戻り、表示は今と同じ
- 同じパスは 1 回だけ写像する（`FONT_FILES`）
- フォントの選ばれ方（family の照会・CJK のカバー判定・index 0 の積み方）は変えない
- 写像とハンドルはプロセスの終わりまで解かない（never-clear）

## テスト方針と検証コマンド

- `font_stack.rs` の `mod tests`（Windows 前提のものは `#[cfg(windows)]`、実フォントが無ければ skip する既存の流儀）
  - 写像の経路を通る: temp（C ドライブ）のファイルを読み込むと `Loaded::Mapped` で、内容が `fs::read` と一致する。**経路を assert しないと、判定が常に写す側へ倒れても全部緑になる**
  - 写像の共有モード: そのハンドルが生きている間、`OpenOptions::new().write(true)` での open が失敗する。`fs::rename` で退避して同じパスへ別の中身を書き、`fs::remove_file` で退避先を消しても、写像の中身は元のバイト列と一致する（A 案の安全性そのものを固定する）
  - 長さ 0 のファイルは `Loaded::Copied` で空のバイト列。存在しないパスは `None`
  - 同じファイルを 2 つの綴り（`/` 区切りと `\` 区切り）で引くと同じ `Arc` を返す（`Arc::ptr_eq`）
  - `maps_on` の表: 固定ドライブで属性なしは真、固定ドライブ以外（リムーバブル・ネットワーク・不明）は偽、プレースホルダ・オフラインの属性があれば偽
  - 実フォント（`C:/Windows/Fonts/YuGothM.ttc` があれば）: `Loaded::Mapped` で、`fs::read` とバイト一致
  - 既存の `font_definitions_*` は `Blob` を受ける形へ追従させ、意味を変えない
- 実測（一時スクリプト・scratchpad）
  - 基準（main）とこの変更で、アイドルの PrivateBytes を測る。構成は 2 つ: 実 config（HackGen）と、検証用プロファイルで `font_family = "Segoe UI"`（jp_font を積む）
  - `npm run bench:startup` の windows_create を基準と交互に（A-B-A）
  - `npm run check:colors` と目視（カテゴリ D）
  - 起動中の Snotra が写像しているフォントファイルの写し（検証用プロファイルで、temp に置いたフォントを font_family に選ぶ）を、PowerShell から書き込みで開くと拒まれ、削除・名前の変更は通り、そのあとも表示が崩れないことを確かめる
- 検証: カテゴリ A・C・D（目視）・E・F

## 作業項目

### Phase 1: 実装
- [x] `Cargo.toml` に `memmap2`
- [x] `font_stack.rs`: `MappedFont`・`Loaded`・`FontFiles`・`load_font_file`（実装では `FontFiles::load` と `map_font_file`・`drive_type_of` に分けた）・`maps_on`・`JP_FONT` / `USER_FONTS` の型・`resolve_font_family`・`font_definitions`。テスト

### Phase 2: 計測と文書
- [x] PrivComm の前後（2 構成）と bench:startup の windows_create を測り、`PERFORMANCE.md` に記録する — アイドル HackGen 20.7/18.4 → 10.6/10.5、Segoe UI+jp 24.3/24.7 → 10.5/8.7。windows_create p50 13・14 → 11・11 ms
- [x] 写像中のフォントファイルへの書き込みが拒まれ、削除・名前の変更の後も表示が崩れないことを実機で確かめる — 削除・置き換えの後も写像の中身が変わらないことは、テスト `mapped_file_refuses_writes_but_survives_delete_and_replace`（%TEMP% の実ファイル）で確かめた。導入済みの実フォント（`HackGenConsole-Regular.ttf`）は消さずに、書き込みで開けるかだけを Snotra の起動前・起動中・終了後に試した。**3 時点とも開けなかった**（Snotra 以外のプロセスが既に書き込みを拒むハンドルを持つ）。導入済みのフォントについては、この変更で新しく増える制約は実質無い
- [x] `check:colors` と目視（カテゴリ D）— 緑。スクリーンショットで jp_font（写像）の和文「検索…」が描かれている。対話の目視 2 点（show の一瞬の色・リサイズのちらつき）はフォントに依らない項目で、人間へ委ねる
- [x] `//!` と doc・`SPEC.md` の 1 文・`PERFORMANCE.md` の現在形の記述を直す
- [x] `/symmetric-check`・`/race-check`（`prefetch` のスレッドと `setup` が同じキャッシュを引く）を当てる — symmetric: 開く/写像 ↔ 解く/閉じるは `MappedFont` の drop（宣言順で写像が先）。キャッシュが never-clear なのでプロセス終了まで解かない（意図）。race（`--base origin/main`・④ 7 件・すべて font_stack.rs の錠）: 境界は prefetch スレッドと setup の 2 者が `USER_FONTS`・`JP_FONT`・`FONT_FILES` を引くことだけ。値は不変の `Arc` で古い結果が生じない（b）、UI 状態を変えないので wake 不要（c）、錠は開く・写像の間も持つので同時に引いても 1 回だけ開く（d）、錠の向きは `USER_FONTS`→`FONT_FILES` と `JP_FONT`→`FONT_FILES` だけで、`resolve_stack` は user の錠を離してから jp を引く（e）→ [安全]
- [x] 検証カテゴリ A・C・E・F

### 検証とレビューのラウンド
- [x] ラウンド 1: アンカー `fb34d3a6` / verify:tree A・C・E・F 緑 / 所見 High 1・Medium 4・Low 5 / 変異 9 のうち生存 3（M5・M7・M2b）。M5 はテストの綴りに大文字小文字を足して塞いだ（`10e22846` で赤を自分で確かめた）。M7（jp を user より先に解決）と M2b（`maps_on` の呼び出しを外す）は残余として受け入れる——前者は main にも順序を見るテストが無く、観測には製品コードへの注入点が要る（`ADR-no-test-only-injection-in-product-code`）。後者はテストの機体に固定ドライブ以外のファイルが無い。採らなかった所見: L2（`FontFiles` の doc の「製品は `font_files` の 1 つを never-clear で使い、テストは自分で作る」は、型にした理由——テストが製品の static を汚さない——の説明として残す）。L1 は assert の文言を直し、判定そのものは残した
- [x] ラウンド 2: アンカー `7bee4762` / verify:tree A・C・E・F 緑 / 前回の所見は解消（N2 を除く）・新しい所見 Low 3 / 変異 2 のうち生存 1（r2-Mb: 正規化の失敗をテストで作れない・残余）。N1 は assert の文言を「読んで写す」へ揃えた。N2 はこの行の L2 の理由を直した。N3 は先例があるので採らない

## 決定の経緯（2026-11-17・試作の実測）

試作（共有モード READ|DELETE で写像・`snotra-1449proto.exe`）と基準（main 77eb5899）を、cache-hit の起動で 2 巡ずつ交互に測った（MiB。値は r1 / r2）。

| 構成 | 段 | 基準 PrivComm | 試作 PrivComm | 基準 WS | 試作 WS |
|---|---|---|---|---|---|
| 実 config（HackGen） | アイドル | 19.9 / 20.5 | 10.5 / 10.5 | 37.3 / 38.4 | 35.6 / 28.1 |
| 実 config（HackGen） | hide 後 | 29.7 / 29.6 | 20.9 / 20.7 | 4.3 / 4.6 | 4.6 / 4.6 |
| Segoe UI + jp_font | アイドル | 23.9 / 24.7 | 10.3 / 10.6 | 41.2 / 42.4 | 28.2 / 28.3 |
| Segoe UI + jp_font | hide 後 | 33.7 / 34.2 | 20.8 / 19.2 | 4.3 / 4.6 | 4.7 / 4.6 |

- 効果は帰属（2026-08-22）の見込みどおり。hide 後も残る commit が下がる
- 共有モードは、`FILE_SHARE_READ` だけ（B）ではなく `| FILE_SHARE_DELETE`（A）を採る（ユーザーの判断）。使い捨てファイルを .NET の写像で開いて実測した: 書き込みで開く → 拒否、削除・名前の変更・退避して同じ名前へ新しい中身を書く → 通り、写像の中身は 3 例とも元の SHA-256 のまま
- B を採らなかった理由: クラッシュの経路（中身の書き換え）は A でも塞がっており、B で上乗せされるのは「削除・名前の変更・置き換えを妨げる」代償だけである

## 未確定（実装前に潰す）

（なし）

## 利用者から見える帰結（PR で名指す）

- Snotra の起動中は、使っているフォントファイル（font_family で選んだフォントと、積んだときの jp_font）を、その場で書き換えられない。削除・名前の変更・置き換えはできる（Snotra は終了まで旧い中身を表示し続ける）。font_family を切り替えた後も、前のフォントは終了まで書き換えを拒まれたまま
- その場で書き換えようとする他のプログラム（フォントのインストーラーなど）は失敗しうる（システムフォントは OS のフォントキャッシュが既に開いている）
- ネットワーク上・クラウド同期のプレースホルダのフォントは写像せず、今と同じく写す

## 人間レビュー

- [x] 承認済み — 2026-11-17 / 問い: "この計画で実装に進めてよろしいですか？" / 回答: "承認する"

## セルフレビュー

- リスク: 高（`unsafe` の写像・プロセス寿命のリソース・共有モードによる外部への影響）
- plan-review: 独立レビュー1体（Step 2b・独立導出。ユーザーの判断は issue のコメントへ記録してから渡した）
- エージェント数: 2（3b の敵対的調査 1・独立導出 1）
- 要対処: 6 件を反映した。(1) 写像してよいかの判定を、パスの文字列ではなく開いたハンドルから行う（`VOLUME_NAME_GUID` + `GetDriveTypeW`・プレースホルダとオフラインの属性も除く・純粋関数 `maps_on`）。(2) `Source::Binary` / `SharedFile` は `Arc` をそのまま使い写さない。(3) キャッシュのキーを正規化パスにする（綴りの違い）。(4) 写像の経路を通ったことを assert するテスト。(5) `SPEC.md` へ書き換えを拒む 1 文。(6) `PERFORMANCE.md` の現在形の記述（never-clear・leak・「1:1 で private commit に載る」）
- 独立レビューの後の差分（2026-11-17）: 共有モードに `FILE_SHARE_DELETE` を足し、受け入れ 4・SAFETY の根拠・共有モードのテスト・SPEC の 1 文・利用者への帰結をそれに合わせた。対象のファイル・シンボル・インターフェースは変わらない。変わった不変条件（削除・置き換えの後も写像の中身が変わらない）は、レビューではなく使い捨てファイルの実測で裏付け、テストで固定する
- 未検証: UNC で `VOLUME_NAME_GUID` が失敗すること（どちらへ転んでも `maps_on` が写さない側へ倒す）
- 散文の全件列挙: 語 6 / 総件数 48 行 / ★未掲載 0 / ☆行未掲載 0（独立導出が別に挙げた `PERFORMANCE.md` の 4 か所と `SPEC.md` 710 行を足した）
- 試行（#1300・高リスクのみ・独立レビュー 1 回ごとに 1 行）: 方式 Step 2b / 設計変更 3（判定をハンドルから・`Source::Binary` を写さない・キーの正規化） / 主の見落とし 3（散文）

# 調査: #1449 フォントのバイト列を、書き込みと削除を拒む共有モードでメモリマップする

## issue の要約

`egui_shell/font_stack.rs` は、フォントのバイト列をヒープへ写して終了まで持つ（user_font は fontdb が開いたデータを `to_vec`、jp_font は `std::fs::read`。どちらも `Box::leak`）。`PERFORMANCE.md`「常駐メモリの帰属」によれば、これがアイドルの private commit の約半分（HackGen 構成で 19.50 MiB 中 10.21 MiB、既定の Segoe UI + jp_font 構成で約 23.8 MiB 中 14.18 MiB）を占める。

方針（2026-11-17・issue のコメントと会話）:
1. まず、バイト列がいまどこに何部あるかを数える
2. 複製があれば所有を 1 つにまとめる
3. それでも足りなければメモリマップを検討する。クラッシュの経路（外からの書き換え）を塞ぐ方法と代償を比べる

**手順 1 の結果（この調査）: プロセスの中のバイト列は、すでに 1 部ずつである。** 手順 2 で減らせるものは無い。ユーザーの判断（2026-11-17・会話）: 「共有モードでマップする」——書き込みと削除を拒む共有モードでファイルを開いてメモリマップし、開けなければ今の複製へ戻る。代償（Snotra の起動中、使っているフォントファイルを更新・削除できない）を受け入れる。

**追記（同日・試作の実測の後）:** 共有モードは `FILE_SHARE_READ | FILE_SHARE_DELETE` へ決め直した（ユーザーの判断「A: 削除も許す」）。書き込みは拒んだまま、削除・名前の変更・置き換えは通り、写像の中身は変わらないことを実測した。測定値と理由は `workspace/plan.md`「決定の経緯（2026-11-17・試作の実測）」。この共有モードで `YuGothM.ttc` と HackGen も開けた（試作の PrivComm が下がったことが根拠）。以下の本文の「削除を拒む」は、この追記の前の記述である。

## 1 部ずつであることの根拠

- user_font: `resolve_font_family` が `fontdb::Database::with_face_data` の中で `data.to_vec()` を 1 回だけ作り、`USER_FONTS`（family 名 → `&'static [u8]`）にキャッシュする。main と results の 2 つの Context は同じキャッシュを引く。`Database` は解決後に drop する
- jp_font: `jp_font_bytes` が `std::fs::read` を 1 回だけ呼び、`JP_FONT` に置く（錠を持ったまま読むので、先読みと `setup` が重なっても 2 度読まない・#1452）
- egui / epaint 0.37: `FontData.font` は `Blob = Arc<dyn AsRef<[u8]> + Send + Sync>`（`epaint/src/text/font_data.rs`）。`FaceStore::install` は `Arc::clone(&font_data.font)`、`FontFace` は `skrifa::FontRef::from_index(blob.as_ref(), ..)` で借用して解析する。中で複製はしない。`FontData::from_blob` は「メモリマップしたシステムフォント」を写さずに使う入口として用意されている

## 関連するファイル・シンボル

- `src-tauri/src/egui_shell/font_stack.rs`
  - `JP_FONT: Mutex<Option<&'static [u8]>>`・`jp_font_bytes`（候補のパス 4 つ: `C:/Windows/Fonts/YuGothM.ttc`・`yugothic.ttf`・`msgothic.ttc`・`meiryo.ttc`）
  - `USER_FONTS: OnceLock<Mutex<HashMap<String, ResolvedFont>>>`・`type ResolvedFont = (&'static [u8], u32)`・`resolve_font_family`
  - `font_covers_cjk(bytes: &[u8], face_index)`・`font_definitions(jp_bytes: Option<&'static [u8]>, user: Option<(&'static [u8], u32)>)`（`FontData::from_static` で積む）・`resolve_stack`・`prefetch`・`configure_japanese_font`
  - テスト: `font_definitions_*`（`mod tests`）。実フォントを読むテストが `C:/Windows/Fonts/YuGothM.ttc` を `std::fs::read` する
  - `//!`: 「バイト列は `Box::leak` で `'static` にし、`JP_FONT` と `USER_FONTS` に 1 度だけ置く（never-clear）」
- `fontdb 0.24`: `Database::face_source(id) -> Option<(Source, u32)>`。`Source::File(PathBuf)` / `Source::Binary(..)` / `Source::SharedFile(..)`。`load_system_fonts` が返すのは通常 `Source::File`
- `memmap2 0.9.12`: `Cargo.lock` にある（fontdb 経由）。`src-tauri/Cargo.toml` には無い（足す）
- `std::os::windows::fs::OpenOptionsExt::share_mode`: `FILE_SHARE_READ`（1）だけを渡すと、開いている間、他のハンドルは書き込みと削除（名前の変更を含む）のために開けない。既に書き込みのために開いているハンドルがあれば、こちらの open が共有違反で失敗する
- `PERFORMANCE.md`「常駐メモリの帰属（2026-08-22 計測・19,985 エントリ・計測コード 3 種）」「採用: user_font も `from_static` で積む」「採用: user_font が CJK をカバーするときの jp_font 省略」
- 計測: `npm run measure:memory:stages`（実 config で Snotra を落として起動し直す）・`npm run bench:startup`・`npm run check:colors`（`scripts/visual-check-colors.ps1`）

## 再利用できる既存のパターン

- `FontData::from_blob(blob, index)`。`Blob` は `Arc` なので、`'static` へ leak する必要が無くなる（キャッシュは `Arc` を持てばよい）
- 失敗時の扱いは今と同じ形にする（user_font は解決失敗で `None`、jp_font は候補を順に試す）。マップできなければ、そのファイルを今の方法（読み込んで写す）で持つ

## 技術的な制約

- `memmap2::Mmap::map` は `unsafe`（写像の背後のファイルが書き換わると UB になりうる）。共有モードで書き込みと削除を拒み、そのハンドルを写像と同じだけ生かすことが、`unsafe` の根拠になる。キャッシュは never-clear なので、写像とハンドルはプロセスの終わりまで生きる
- 長さ 0 のファイルはマップできない（`Mmap::map` が失敗する）→ 写す方法へ戻る
- user_font の解決は fontdb の照会（family 名 → face）を使い続ける。バイト列は `face_source` のパスから自分で開く。`Source::File` 以外（`Binary` / `SharedFile`）は今の `with_face_data` + 写しへ戻る
- フォント登録に触る変更なので、`docs/build-commands.md` カテゴリ D の目視スモークを省かない（`font_stack.rs` の `//!`）
- 写像したページは private commit ではなく共有・ファイル裏付けになる。working set には触ったぶんだけ載る

## 敵対的調査（`workspace/adversarial-1449.txt`）の採否

壊せなかった: 命題 1（1 部ずつ）・2（共有モードで書き込み・削除・名前の変更が拒まれる。使い捨てファイルで実測）・3（`YuGothM.ttc`・`msgothic.ttc`・`meiryo.ttc`・`segoeui.ttf`・`HackGenConsole-Regular.ttf` を Read/Share=Read で開けた。`yugothic.ttf` は存在しない）・4（`load_system_fonts` は `Source::File`）・5（`measure-memory.ps1` の PrivateBytes / WorkingSetPrivate は共有のファイル裏付けを含まない。読み取り専用の写像の private 増分は実質 0、`ReadAllBytes` は +10.3 MiB）。

- **採る: 「1 部ずつ」は役割ごとで、ファイルごとではない。** 同じファイルへ解決する別名へ切り替えると、family 名のキーでは別に写す。user_font と jp_font が同じファイル（`YuGothM.ttc`）なら 2 本常駐する。写像のキャッシュをファイルのパスで持てば消える
- **採る: ハンドルは写像と一緒に持つ。** memmap2 0.9.12 の Windows 実装は `DuplicateHandle` で自前のハンドルを持つので `File` を drop しても共有モードは保たれるが、版に依存する。`unsafe` の根拠を自分のコードに置くため、`File` も持つ
- **採る: ネットワーク上のファイルは写像しない。** ページの読み込みが失敗すると `EXCEPTION_IN_PAGE_ERROR` で落ちる（release は panic=abort で捕まえられない）。fontdb は Roaming も走査し、リダイレクトされうる。写像するのはローカルの固定ドライブ（`GetDriveTypeW` が `DRIVE_FIXED`）上のファイルだけにする
- **採る: 代償は「更新・削除できない」より広い。** 排他で開こうとする他のプログラム（フォントのインストーラー・Windows Update のサービシング・AV）も失敗しうる。PR で名指す
- **採る: 帰属の実測（2026-08-22）は egui 0.36 の時代。** 0.37 での前後を測る
- 機体の前提: 実 config は `font_family = "HackGen Console"`（ユーザーが入れたフォント・CJK をカバーするので jp_font は積まれない）。この機体の実 config で下がるのは 10.21 MiB の 1 本。jp_font の構成は検証用プロファイル（`font_family = "Segoe UI"`）で測る
- ⚠️ peniko / vello_cpu / skrifa の内部の写しは全読みしていない → 0.37 の実測で裏付ける

## 未解決の疑問

- システムフォント（`C:/Windows/Fonts/YuGothM.ttc`）を `FILE_SHARE_READ` だけで開けるか。Windows のフォントキャッシュサービスが書き込みアクセスのハンドルを持っていると共有違反になり、毎回写す方法へ戻る（効果が出ない）
- ユーザーが入れたフォント（`%LOCALAPPDATA%\Microsoft\Windows\Fonts`）も同じく開けるか
- アイドルの PrivComm が実際にどれだけ下がるか（見込みは帰属からの導出）
- 表示後の最初のラスタ化で、ページの読み込みが体感に出るか

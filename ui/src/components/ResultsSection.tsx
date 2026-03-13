import { type Component, For, createSignal, createEffect, createMemo, on, onMount, onCleanup } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
import { results, selected, getSearchGeneration } from "../stores/search";
import * as api from "../lib/invoke";
import { LruIconCache } from "../lib/lruIconCache";
import { perfMarkRenderDone } from "../lib/perf";
import ResultRow from "./ResultRow";

export interface ResultsSectionProps {
  visible: boolean;
  showIcons: boolean;
  skipIcons: boolean;
  maxResults: number;
  onClickResult: (index: number) => void;
  onDoubleClickResult: (index: number) => void;
}

const ResultsSection: Component<ResultsSectionProps> = (props) => {
  const iconCache = new LruIconCache();
  // アイコン取得を試みたが存在しなかったパス（フォールバック絵文字を表示）
  const fetchedNone = new Set<string>();
  const [iconCacheVersion, setIconCacheVersion] = createSignal(0);
  const [containerWidth, setContainerWidth] = createSignal(0);
  const [font, setFont] = createSignal("15px 'Segoe UI'");
  let listRef: HTMLDivElement | undefined;
  let latestIconRequestId = 0;
  let lastScrolledSelected = -1;
  let lastScrolledGeneration = -1;
  let iconRequestId = 0;   // アイコン取得の世代カウンタ（staleness guard 用）
  let listGeneration = 0;  // スクロール追従の世代カウンタ（results 変化時のみ更新）

  function ensureRowVisible(container: HTMLDivElement, row: HTMLElement) {
    const cRect = container.getBoundingClientRect();
    const rRect = row.getBoundingClientRect();

    if (rRect.top < cRect.top) {
      container.scrollTop -= cRect.top - rRect.top;
      return;
    }
    if (rRect.bottom > cRect.bottom) {
      container.scrollTop += rRect.bottom - cRect.bottom;
    }
  }

  /** Parse length-prefixed binary batch into per-path Blob URLs.
   *  Format: [count:u32 LE] then per icon: [status:u8] [if 1: png_len:u32 LE, png_bytes] */
  function parseBinaryBatch(
    buf: ArrayBuffer,
    paths: string[],
  ): Map<string, string> {
    const view = new DataView(buf);
    let offset = 0;
    const count = view.getUint32(offset, true);
    offset += 4;
    const result = new Map<string, string>();
    for (let i = 0; i < count; i++) {
      const status = view.getUint8(offset);
      offset += 1;
      if (status === 1) {
        const pngLen = view.getUint32(offset, true);
        offset += 4;
        const pngBytes = new Uint8Array(buf, offset, pngLen);
        offset += pngLen;
        const blob = new Blob([pngBytes], { type: "image/png" });
        const url = URL.createObjectURL(blob);
        result.set(paths[i], url);
      }
    }
    return result;
  }

  /** キャッシュにないアイコンを一括取得してキャッシュに格納する。stale なら Blob URL を破棄して早期リターン */
  async function fetchIconBatch(items: SearchResult[], generation: number): Promise<void> {
    const missing = items
      .filter((r) => !r.isError && !iconCache.has(r.path) && !fetchedNone.has(r.path))
      .map((r) => r.path);
    if (missing.length === 0) return;

    let parsed: Map<string, string>;
    try {
      const buf = await api.getIconsBatch(missing);
      parsed = parseBinaryBatch(buf, missing);
    } catch (e) {
      console.warn("fetchIcons failed:", e);
      return;
    }
    if (generation !== latestIconRequestId) {
      for (const url of parsed.values()) {
        URL.revokeObjectURL(url);
      }
      return;
    }

    for (const [path, url] of parsed) {
      iconCache.set(path, url);
    }
    // 取得できなかったパスをマーク（次回の filter でスキップ、かつフォールバック絵文字を表示）
    for (const path of missing) {
      if (!parsed.has(path)) fetchedNone.add(path);
    }
    setIconCacheVersion((v) => v + 1);
  }

  async function fetchIcons(items: SearchResult[], generation: number) {
    // RAF スケジュール後に条件が変わった場合の早期脱出（skipIcons は取得抑制のみ・破棄しない）
    if (!iconsEnabled() || props.skipIcons) return;
    // 呼び出し時点の値を固定し、await 中の設定変更でスライス境界がずれるのを防ぐ
    const visibleCount = props.maxResults;
    // 可視行を先に await し、世代が変わっていなければ非表示行を開始する。
    // 逐次実行にすることで、連続入力で世代がすぐ変わるケースに
    // offscreen バッチ（Rust 側の rayon 処理を含む）を開始せずに済む。
    await fetchIconBatch(items.slice(0, visibleCount), generation);
    if (generation !== latestIconRequestId) return;
    if (items.length > visibleCount) {
      await fetchIconBatch(items.slice(visibleCount), generation);
    }
  }

  // アイコン取得条件を一元管理するメモ: 3 props の組み合わせが変化したときのみ再評価
  const iconsEnabled = createMemo(() => props.visible && props.showIcons);

  // アイコンライフサイクル: results または iconsEnabled が変化したとき取得開始 / キャッシュ破棄
  createEffect(on([results, iconsEnabled] as const, ([items, enabled]) => {
    if (!enabled) {
      latestIconRequestId = ++iconRequestId;
      iconCache.revokeAll();
      fetchedNone.clear();
      setIconCacheVersion((v) => v + 1);
      return;
    }
    const id = ++iconRequestId;
    latestIconRequestId = id;
    fetchedNone.clear();
    requestAnimationFrame(() => void fetchIcons(items, id));
  }));

  // results 専用: perf 計測 + スクロール世代更新（アイコン条件とは独立）
  createEffect(on(results, (items) => {
    ++listGeneration;
    if (items.length > 0) {
      const perfRequestId = getSearchGeneration();
      requestAnimationFrame(() => {
        perfMarkRenderDone(perfRequestId);
      });
    }
  }));

  // selected または results が変化した時: スクロール追従
  createEffect(() => {
    const sel = selected();
    scrollToSelected(sel, listGeneration);
  });

  onMount(() => {
    const unlistenFns: Array<() => void> = [];
    onCleanup(() => {
      for (const fn of unlistenFns) fn();
      iconCache.revokeAll();
    });

    // show-icons-changed は MainApp が setShowIcons() で受け取り props.showIcons に流す。
    // iconsEnabled メモ経由でアイコンライフサイクル effect が反応するため、ここでは listen 不要。
    void (async () => {
      const unlistenVisualFont = await listen("visual-config-changed", () => {
        if (listRef) {
          const style = getComputedStyle(listRef);
          setFont(`${style.fontSize} ${style.fontFamily}`);
        }
      });
      unlistenFns.push(unlistenVisualFont);
    })().catch((e) => console.warn("ResultsSection: failed to setup listeners:", e));

    // Measure font once at list level for all ResultRow instances
    if (listRef) {
      const style = getComputedStyle(listRef);
      setFont(`${style.fontSize} ${style.fontFamily}`);
    }

    if (listRef) {
      const ro = new ResizeObserver((entries) => {
        for (const entry of entries) {
          setContainerWidth(entry.contentRect.width);
        }
      });
      ro.observe(listRef);
      onCleanup(() => ro.disconnect());
    }
  });

  function scrollToSelected(selectedIdx: number, generation: number) {
    if (selectedIdx !== lastScrolledSelected || generation !== lastScrolledGeneration) {
      lastScrolledSelected = selectedIdx;
      lastScrolledGeneration = generation;
      queueMicrotask(() => {
        if (!listRef) return;
        const row = listRef.children[selectedIdx] as HTMLElement | undefined;
        if (!row) return;
        ensureRowVisible(listRef, row);
      });
    }
  }

  return (
    <div class="results-section">
      <div class="result-list-standalone" ref={listRef} role="listbox" aria-label="検索結果">
        <For each={results()}>
          {(result, idx) => (
            <ResultRow
              result={result}
              isSelected={idx() === selected()}
              icon={(iconCacheVersion(), result.isError ? null : iconCache.get(result.path) ?? (fetchedNone.has(result.path) ? null : undefined))}
              showIcons={props.showIcons && !props.skipIcons}
              containerWidth={containerWidth()}
              font={font()}
              onClick={() => props.onClickResult(idx())}
              onDoubleClick={() => props.onDoubleClickResult(idx())}
            />
          )}
        </For>
      </div>
    </div>
  );
};

export default ResultsSection;

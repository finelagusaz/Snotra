import { type Component, For, createSignal, createEffect, createMemo, createSelector, on, onMount, onCleanup } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { listen } from "@tauri-apps/api/event";
import type { SearchResult } from "../lib/types";
import { results, selected, getSearchGeneration } from "../stores/search";
import * as api from "../lib/invoke";
import { LruIconCache } from "../lib/lruIconCache";
import { parseBinaryBatch } from "../lib/iconBatch";
import { perfMarkRenderDone } from "../lib/perf";
import { clearTruncateCaches } from "../lib/truncatePath";
import ResultRow from "./ResultRow";

export interface ResultsSectionProps {
  visible: boolean;
  showIcons: boolean;
  skipIcons: boolean;
  maxResults: number;
  iconCacheSize: number;
  onClickResult: (index: number) => void;
  onDoubleClickResult: (index: number) => void;
}

const ResultsSection: Component<ResultsSectionProps> = (props) => {
  const iconCache = new LruIconCache(props.iconCacheSize);
  // アイコン取得を試みたが存在しなかったパス（フォールバック絵文字を表示）
  const fetchedNone = new Set<string>();
  // per-path のリアクティブ通知: iconNotify[path] を読むことで、
  // そのパスのアイコンが変化したときだけ該当行が再評価される。
  // iconCacheVersion（全行ブロードキャスト）を廃止し O(変更行数) に改善。
  const [iconNotify, setIconNotify] = createStore<Record<string, number>>({});
  let iconNotifyCounter = 0;
  const [containerWidth, setContainerWidth] = createSignal(0);
  const [font, setFont] = createSignal("15px 'Segoe UI'");
  let listRef: HTMLDivElement | undefined;
  let latestIconRequestId = 0;
  let lastScrolledSelected = -1;
  let lastScrolledGeneration = -1;
  let iconRequestId = 0;   // アイコン取得の世代カウンタ（staleness guard 用）
  let listGeneration = 0;  // スクロール追従の世代カウンタ（results 変化時のみ更新）
  // createSelector: selected() が変化したとき、前回値と今回値の2行だけに通知する。
  // 全行が selected() を購読する isSelected={idx() === selected()} と異なり O(1) 更新。
  const isRowSelected = createSelector(selected);

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

    const v = ++iconNotifyCounter;
    for (const [path, url] of parsed) {
      iconCache.set(path, url);
      setIconNotify(path, v);
    }
    // 取得できなかったパスをマーク（次回の filter でスキップ、かつフォールバック絵文字を表示）
    for (const path of missing) {
      if (!parsed.has(path)) {
        fetchedNone.add(path);
        setIconNotify(path, v);
      }
    }
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

  // アイコンライフサイクル: results / iconsEnabled / skipIcons のいずれかが変化したとき
  // enabled=false → キャッシュ破棄（visible 非表示・アイコン無効）
  // skip=true     → 何もしない（IC モード中: キャッシュ保持・取得抑制）
  // enabled かつ非 skip → 取得開始（results 変化・再表示・skipIcons 復帰すべてをカバー）
  createEffect(on([results, iconsEnabled, () => props.skipIcons] as const, ([items, enabled, skip], prev) => {
    if (!enabled) {
      latestIconRequestId = ++iconRequestId;
      iconCache.revokeAll();
      fetchedNone.clear();
      setIconNotify(reconcile({}));
      return;
    }
    if (skip) return;
    const id = ++iconRequestId;
    latestIconRequestId = id;
    // results が変化したときのみクリア。skipIcons 切替だけの場合は「アイコンなし」確定済み情報を保持する
    if (prev === undefined || prev[0] !== items) {
      fetchedNone.clear();
    }
    requestAnimationFrame(() => void fetchIcons(items, id));
  }));

  // iconCacheSize 変更時: キャッシュを全クリアしてサイズを更新する。
  // setMaxSize だけでは evict されたアイコンの <img src="blob:revoked"> が
  // 壊れたまま残る（iconCacheVersion が更新されないため）。
  // revokeAll + version bump で「全アイコン再描画 → 次回検索で再取得」にする。
  createEffect(on(() => props.iconCacheSize, (size) => {
    latestIconRequestId = ++iconRequestId;
    iconCache.setMaxSize(size);
    iconCache.revokeAll();
    fetchedNone.clear();
    setIconNotify(reconcile({}));
  }, { defer: true }));

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
  // results を依存に含めることで、selected=0→0 のまま結果が変わった場合もスクロールが発火する。
  createEffect(on([results, selected], ([, sel]) => {
    scrollToSelected(sel, listGeneration);
  }));

  onMount(() => {
    const unlistenFns: Array<() => void> = [];
    let cleaned = false;
    onCleanup(() => {
      cleaned = true;
      for (const fn of unlistenFns) fn();
      iconCache.revokeAll();
    });

    // show-icons-changed は MainApp が setShowIcons() で受け取り props.showIcons に流す。
    // iconsEnabled メモ経由でアイコンライフサイクル effect が反応するため、ここでは listen 不要。
    void (async () => {
      const unlistenVisualFont = await listen("visual-config-changed", () => {
        clearTruncateCaches();
        if (listRef) {
          const style = getComputedStyle(listRef);
          setFont(`${style.fontSize} ${style.fontFamily}`);
        }
      });
      if (cleaned) {
        unlistenVisualFont();
        return;
      }
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
        row.scrollIntoView({ block: "nearest", inline: "nearest" });
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
              isSelected={isRowSelected(idx())}
              icon={(iconNotify[result.path], result.isError ? null : iconCache.peek(result.path) ?? (fetchedNone.has(result.path) ? null : undefined))}
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

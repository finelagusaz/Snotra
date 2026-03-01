import { type Component, Show, createSignal, createMemo, createEffect, onMount } from "solid-js";
import type { SearchResult } from "../lib/types";
import { truncatePath } from "../lib/truncatePath";

interface ResultRowProps {
  result: SearchResult;
  isSelected: boolean;
  containerWidth?: number;
  onClick: () => void;
  onDoubleClick: () => void;
  onMouseEnter?: () => void;
}

const ResultRow: Component<ResultRowProps> = (props) => {
  let textRef: HTMLDivElement | undefined;
  const [font, setFont] = createSignal("15px 'Segoe UI'");
  // Track load failure so the emoji fallback can be shown when the protocol returns 404.
  const [iconErrored, setIconErrored] = createSignal(false);

  onMount(() => {
    if (textRef) {
      const style = getComputedStyle(textRef);
      setFont(`${style.fontSize} ${style.fontFamily}`);
    }
  });

  // Reset error state when the result path changes (row reuse by SolidJS For).
  createEffect(() => {
    // Track props.result.path reactively; any change resets the errored flag.
    void props.result.path;
    setIconErrored(false);
  });

  const fullPath = createMemo(() => {
    const p = props.result.path;
    return props.result.isFolder && !p.endsWith("\\") ? p + "\\" : p;
  });

  const displayPath = createMemo(() => {
    void props.containerWidth; // resize trigger
    const f = font();
    if (!textRef) return fullPath();
    const w = textRef.clientWidth;
    if (w === 0) return fullPath();
    return truncatePath(fullPath(), w, f);
  });

  // Build snotra-icon:// URL for file results; folders/errors always fall back to emoji.
  const iconUrl = createMemo(() => {
    if (props.result.isError || props.result.isFolder) return null;
    return `snotra-icon://localhost?path=${encodeURIComponent(props.result.path)}`;
  });

  // Show the <img> only when we have a URL and it hasn't errored.
  const showIcon = createMemo(() => iconUrl() !== null && !iconErrored());

  return (
    <div
      class="result-row"
      classList={{ selected: props.isSelected, error: props.result.isError }}
      onClick={props.onClick}
      onDblClick={props.onDoubleClick}
      onMouseEnter={props.onMouseEnter}
    >
      <div class="result-icon">
        <Show
          when={showIcon()}
          fallback={
            <span class="icon-fallback">
              {props.result.isError
                ? "\u26A0\uFE0F"
                : props.result.isFolder
                  ? "\u{1F4C1}"
                  : "\u{1F4C4}"}
            </span>
          }
        >
          <img
            src={iconUrl()!}
            alt=""
            width="16"
            height="16"
            onError={() => setIconErrored(true)}
          />
        </Show>
      </div>
      <div class="result-text" ref={textRef}>
        <div class="result-path-single">{displayPath()}</div>
      </div>
    </div>
  );
};

export default ResultRow;

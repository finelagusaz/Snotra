import { type Component, Show, createSignal, createMemo, onMount } from "solid-js";
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

  onMount(() => {
    if (textRef) {
      const style = getComputedStyle(textRef);
      setFont(`${style.fontSize} ${style.fontFamily}`);
    }
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

  // For non-error files, derive the icon URL from the file path via custom protocol.
  // Folders and error rows use the emoji fallback path.
  const iconUrl = createMemo(() => {
    if (props.result.isError || props.result.isFolder) return null;
    return `snotra-icon://localhost?path=${encodeURIComponent(props.result.path)}`;
  });

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
          when={iconUrl()}
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
          {(url) => (
            <img
              src={url()}
              alt=""
              width="16"
              height="16"
              onError={(e) => {
                // Hide broken image; Show's fallback slot handles the emoji.
                (e.currentTarget as HTMLImageElement).style.display = "none";
              }}
            />
          )}
        </Show>
      </div>
      <div class="result-text" ref={textRef}>
        <div class="result-path-single">{displayPath()}</div>
      </div>
    </div>
  );
};

export default ResultRow;

import { type Component, Show, createSignal, createMemo, onMount } from "solid-js";
import type { SearchResult } from "../lib/types";
import { truncatePath } from "../lib/truncatePath";

interface ResultRowProps {
  result: SearchResult;
  isSelected: boolean;
  icon?: string;
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

  return (
    <div
      class="result-row"
      classList={{ selected: props.isSelected }}
      onClick={props.onClick}
      onDblClick={props.onDoubleClick}
      onMouseEnter={props.onMouseEnter}
    >
      <div class="result-icon">
        <Show
          when={props.icon}
          fallback={
            <span class="icon-fallback">
              {props.result.isFolder ? "\u{1F4C1}" : "\u{1F4C4}"}
            </span>
          }
        >
          <img
            src={`data:image/png;base64,${props.icon}`}
            alt=""
            width="16"
            height="16"
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

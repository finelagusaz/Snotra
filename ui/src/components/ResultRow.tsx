import { type Component, Show, createMemo, mergeProps } from "solid-js";
import type { SearchResult } from "../lib/types";
import { truncatePath } from "../lib/truncatePath";

interface ResultRowProps {
  result: SearchResult;
  isSelected: boolean;
  /** undefined = 未取得（空表示）, null = 取得完了・アイコンなし（フォールバック絵文字）, string = Blob URL */
  icon?: string | null;
  showIcons?: boolean;
  containerWidth?: number;
  font: string;
  onClick: () => void;
  onDoubleClick: () => void;
}

const ResultRow: Component<ResultRowProps> = (rawProps) => {
  const props = mergeProps({ showIcons: true }, rawProps);
  let textRef: HTMLDivElement | undefined;

  const fullPath = createMemo(() => {
    const p = props.result.path;
    return props.result.isFolder && !p.endsWith("\\") ? p + "\\" : p;
  });

  const displayPath = createMemo(() => {
    void props.containerWidth; // resize trigger
    if (!textRef) return fullPath();
    const w = textRef.clientWidth;
    if (w === 0) return fullPath();
    return truncatePath(fullPath(), w, props.font);
  });

  return (
    <div
      class="result-row"
      role="option"
      aria-selected={props.isSelected}
      classList={{ selected: props.isSelected, error: props.result.isError }}
      onClick={props.onClick}
      onDblClick={props.onDoubleClick}
    >
      <Show when={props.showIcons}>
        <div class="result-icon">
          <Show when={props.icon !== undefined}>
            <Show
              when={props.icon}
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
                src={props.icon!}
                alt=""
                width="16"
                height="16"
              />
            </Show>
          </Show>
        </div>
      </Show>
      <div class="result-text" ref={textRef}>
        <div class="result-path-single">{displayPath()}</div>
      </div>
    </div>
  );
};

export default ResultRow;

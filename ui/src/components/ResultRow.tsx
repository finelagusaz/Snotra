import { type Component, Show } from "solid-js";
import type { SearchResult } from "../lib/types";

interface ResultRowProps {
  result: SearchResult;
  isSelected: boolean;
  icon?: string;
  onClick: () => void;
  onDoubleClick: () => void;
  onMouseEnter?: () => void;
}

const ResultRow: Component<ResultRowProps> = (props) => {
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
      <div class="result-text">
        <div class="result-name">
          {props.result.isFolder && <span class="folder-badge">[DIR]</span>}
          {props.result.name}
        </div>
        <div class="result-path">{props.result.path}</div>
      </div>
    </div>
  );
};

export default ResultRow;

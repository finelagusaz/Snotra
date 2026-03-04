import { Show, type ParentComponent } from "solid-js";
import { t } from "../lib/i18n";

type SettingsEditableListProps = {
  hasItems: boolean;
  emptyMessage: string;
  addLabel?: string;
  onAdd: () => void;
};

const SettingsEditableList: ParentComponent<SettingsEditableListProps> = (props) => {
  return (
    <>
      <div class="scan-path-list settings-editor-list">
        <Show
          when={props.hasItems}
          fallback={
            <div class="scan-path-list-empty settings-editor-list-empty">
              {props.emptyMessage}
            </div>
          }
        >
          {props.children}
        </Show>
      </div>
      <div class="scan-path-list-actions settings-editor-list-actions">
        <button type="button" onClick={props.onAdd}>
          {props.addLabel ?? t("common.add")}
        </button>
      </div>
    </>
  );
};

export default SettingsEditableList;

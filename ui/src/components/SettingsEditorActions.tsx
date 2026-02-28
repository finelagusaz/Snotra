import type { Component, JSX } from "solid-js";

type SettingsEditorActionsProps = {
  left?: JSX.Element;
  right: JSX.Element;
};

const SettingsEditorActions: Component<SettingsEditorActionsProps> = (props) => {
  return (
    <div class="settings-editor-actions">
      {props.left}
      <div class="settings-editor-actions-primary">{props.right}</div>
    </div>
  );
};

export default SettingsEditorActions;

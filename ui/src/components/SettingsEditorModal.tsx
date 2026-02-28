import { Show, createEffect, type ParentComponent } from "solid-js";

type SettingsEditorModalProps = {
  open: boolean;
  title: string;
  titleId: string;
  onClose: () => void;
  initialFocusEl?: () => HTMLElement | undefined;
};

const SettingsEditorModal: ParentComponent<SettingsEditorModalProps> = (props) => {
  createEffect(() => {
    if (!props.open) return;
    queueMicrotask(() => {
      const el = props.initialFocusEl?.();
      if (!el) return;
      el.focus();
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
        el.select();
      }
    });
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    props.onClose();
  }

  return (
    <Show when={props.open}>
      <div class="settings-modal-backdrop" onClick={props.onClose}>
        <div
          class="settings-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby={props.titleId}
          tabIndex={-1}
          onClick={(e) => e.stopPropagation()}
          onKeyDown={handleKeyDown}
        >
          <div class="settings-modal-header">
            <div id={props.titleId} class="settings-modal-title">
              {props.title}
            </div>
            <button
              type="button"
              class="settings-modal-close"
              onClick={props.onClose}
              aria-label="閉じる"
            >
              x
            </button>
          </div>

          {props.children}
        </div>
      </div>
    </Show>
  );
};

export default SettingsEditorModal;

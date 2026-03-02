import { Show, createEffect, type ParentComponent } from "solid-js";

type SettingsEditorModalProps = {
  open: boolean;
  title: string;
  titleId: string;
  onClose: () => void;
  initialFocusEl?: () => HTMLElement | undefined;
};

const SettingsEditorModal: ParentComponent<SettingsEditorModalProps> = (props) => {
  let modalRef: HTMLDivElement | undefined;

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

  function getFocusable(): HTMLElement[] {
    if (!modalRef) return [];
    return Array.from(
      modalRef.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])'
      )
    ).filter((el) => el.offsetParent !== null);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      props.onClose();
      return;
    }
    if (e.key === "Tab") {
      const focusable = getFocusable();
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey) {
        if (document.activeElement === first) {
          e.preventDefault();
          last.focus();
        }
      } else {
        if (document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }
  }

  return (
    <Show when={props.open}>
      <div class="settings-modal-backdrop" onClick={props.onClose}>
        <div
          class="settings-modal"
          ref={modalRef}
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
              ×
            </button>
          </div>

          {props.children}
        </div>
      </div>
    </Show>
  );
};

export default SettingsEditorModal;

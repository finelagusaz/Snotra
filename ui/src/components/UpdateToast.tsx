import { type Component, Show } from "solid-js";
import { t } from "../lib/i18n";

interface Props {
  version: string;
  canInstall: boolean;
  installing: boolean;
  error: boolean;
  errorDetail: string | null;
  onInstall: () => void;
  onDismiss: () => void;
}

const UpdateToast: Component<Props> = (props) => {
  return (
    <div class="update-toast">
      <div class="update-toast-row1">
        <span class="update-toast-text">
          {props.error
            ? `${t("update.failed")}${props.errorDetail ? `: ${props.errorDetail}` : ""}`
            : props.installing
              ? t("update.installing")
              : t("update.available", { version: props.version })}
        </span>
      </div>
      <div class="update-toast-row2">
        <Show when={props.canInstall}>
          <button
            class="update-toast-btn-install"
            disabled={props.installing}
            onClick={props.onInstall}
          >
            {t("update.install_now")}
          </button>
        </Show>
        <button
          class="update-toast-btn-dismiss"
          disabled={props.installing}
          onClick={props.onDismiss}
        >
          {t("update.dismiss")}
        </button>
      </div>
    </div>
  );
};

export default UpdateToast;

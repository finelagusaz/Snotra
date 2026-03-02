import { type Component, onMount, onCleanup } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-shell";

const AboutWindow: Component = () => {
    const rawVersion = import.meta.env.VITE_APP_VERSION || "0.9";
    const version = rawVersion.startsWith("v") ? rawVersion.slice(1) : rawVersion;
    const buildDate = import.meta.env.VITE_APP_BUILD_NUMBER || "20260223";

    onMount(() => {
        const handler = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                e.preventDefault();
                void getCurrentWindow().close();
            }
        };
        window.addEventListener("keydown", handler);
        onCleanup(() => window.removeEventListener("keydown", handler));
    });

    return (
        <div
            style={{
                padding: "24px",
                color: "var(--text-color)",
                "font-family": "var(--font-family)",
                height: "100%",
                display: "flex",
                "flex-direction": "column",
                background: "var(--bg-color)",
                "user-select": "text",
            }}
        >
            <h2 style={{ "margin-top": "0", "margin-bottom": "8px", "color": "var(--text-color)" }}>Snotra</h2>
            <p style={{ margin: "0 0 24px 0", color: "var(--hint-text-color)", "font-size": "14px" }}>
                v{version} build {buildDate}
            </p>

            <div style={{ "margin-top": "auto", "font-size": "14px", "line-height": "1.6" }}>
                <p style={{ margin: "0 0 8px 0" }}>Fine Lagusaz</p>
                <p style={{ margin: "0 0 8px 0" }}>
                    <a
                        href="#"
                        style={{ color: "var(--text-color)", "text-decoration": "none" }}
                        onClick={(e) => {
                            e.preventDefault();
                            open(`mailto:algiz.rune@gmail.com?subject=${encodeURIComponent("Snotraについて")}`);
                        }}
                    >
                        algiz.rune@gmail.com
                    </a>
                </p>
                <p style={{ margin: "0" }}>
                    <a
                        href="#"
                        style={{ color: "var(--text-color)", "text-decoration": "none" }}
                        onClick={(e) => {
                            e.preventDefault();
                            open("https://blankrune.sakura.ne.jp/");
                        }}
                    >
                        https://blankrune.sakura.ne.jp/
                    </a>
                </p>
            </div>
        </div>
    );
};

export default AboutWindow;

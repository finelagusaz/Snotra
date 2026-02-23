import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import process from "process";
import packageJson from "./package.json";

// 開発（ローカル）時にプロセス環境変数が無い場合、YYYYMMDD形式の日付をフォールバックとして設定
const fallbackDate = new Date().toISOString().split('T')[0].replace(/-/g, '');

export default defineConfig({
  plugins: [solid()],
  root: "ui",
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    target: "esnext",
    outDir: "../dist",
  },
  define: {
    'import.meta.env.VITE_APP_VERSION': JSON.stringify(process.env.VITE_APP_VERSION || packageJson.version),
    'import.meta.env.VITE_APP_BUILD_NUMBER': JSON.stringify(process.env.VITE_APP_BUILD_NUMBER || fallbackDate),
  },
});

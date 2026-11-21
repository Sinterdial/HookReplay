/**
 * Vite 构建配置
 * 配置 Vue 3 与 Tailwind CSS v4 插件，并遵循 Tauri v2 桌面端静态资源与端口约定。
 */
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";

// Tauri 在开发环境固定监听 1420 端口；生产打包则输出纯静态 dist/ 目录
export default defineConfig({
  plugins: [vue(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // 忽略 Rust 后端目录变更，避免不必要的 Vite 热更新抖动
      ignored: ["**/src-tauri/**"]
    }
  }
});

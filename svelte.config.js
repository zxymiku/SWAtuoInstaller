import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** Svelte 5 编译配置：使用 vitePreprocess 支持 <script lang="ts">。 */
export default {
  preprocess: vitePreprocess(),
  compilerOptions: {
    runes: true,
  },
};

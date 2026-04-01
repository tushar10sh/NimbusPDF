import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({ out: 'build' }),
    alias: {
      $api: 'src/lib/api',
      $components: 'src/lib/components',
      $stores: 'src/lib/stores',
    },
  },
};

export default config;

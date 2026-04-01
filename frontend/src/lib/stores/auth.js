import { writable } from 'svelte/store';
import { api } from '$api/client.js';

function createAuthStore() {
  const { subscribe, set } = writable({
    user: null,
    loading: true,
  });

  return {
    subscribe,
    async init() {
      try {
        const me = await api.get('/auth/me');
        set({ user: me, loading: false });
      } catch {
        set({ user: null, loading: false });
      }
    },
  };
}

export const auth = createAuthStore();

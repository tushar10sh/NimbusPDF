<!-- AI endpoint configuration + optional Google Drive setup -->
<script>
  import { onMount } from 'svelte';
  import { api } from '$api/client.js';
  import { auth } from '$stores/auth.js';

  let config = { endpoint_url: '', model: '', api_key: '' };
  let saved = false;
  let error = null;

  onMount(async () => {
    try {
      config = await api.get('/ai/config');
    } catch {}
  });

  async function saveConfig() {
    try {
      await api.post('/ai/config', config);
      saved = true;
      setTimeout(() => (saved = false), 2000);
    } catch (e) {
      error = e.message;
    }
  }
</script>

<main class="max-w-xl mx-auto p-8">
  <h1 class="text-xl font-bold mb-6">Settings</h1>

  <section class="bg-white rounded-lg shadow p-6 mb-6">
    <h2 class="font-semibold mb-4">AI Endpoint</h2>
    <label class="block mb-3">
      <span class="text-sm text-gray-600">Endpoint URL</span>
      <input bind:value={config.endpoint_url} class="mt-1 block w-full border rounded px-3 py-2 text-sm"
        placeholder="http://localhost:11434/v1/chat/completions" />
    </label>
    <label class="block mb-3">
      <span class="text-sm text-gray-600">Model</span>
      <input bind:value={config.model} class="mt-1 block w-full border rounded px-3 py-2 text-sm"
        placeholder="llama3" />
    </label>
    <label class="block mb-4">
      <span class="text-sm text-gray-600">API Key (optional)</span>
      <input type="password" bind:value={config.api_key} class="mt-1 block w-full border rounded px-3 py-2 text-sm"
        placeholder="sk-…" />
    </label>
    <button on:click={saveConfig} class="bg-blue-600 text-white px-4 py-2 rounded text-sm hover:bg-blue-700">
      {saved ? 'Saved!' : 'Save'}
    </button>
    {#if error}<p class="text-red-500 text-sm mt-2">{error}</p>{/if}
  </section>

  {#if $auth.user}
    <section class="bg-white rounded-lg shadow p-6">
      <h2 class="font-semibold mb-4">Google Drive Sync</h2>
      {#if $auth.user.gdrive_connected}
        <p class="text-sm text-green-600 mb-3">Connected</p>
        <a href="/api/auth/gdrive/disconnect" class="text-sm text-red-500">Disconnect</a>
      {:else}
        <a href="/api/auth/gdrive" class="bg-white border px-4 py-2 rounded text-sm hover:bg-gray-50">
          Connect Google Drive
        </a>
      {/if}
    </section>
  {/if}
</main>

<script>
  import MemoryEditor from '$components/MemoryEditor.svelte';
  import { auth } from '$stores/auth.js';
</script>

<main class="max-w-4xl mx-auto p-8">
  <div class="flex items-center gap-4 mb-6">
    <a href="/" class="text-sm text-gray-500 hover:text-gray-900">← Library</a>
    <h1 class="text-xl font-bold">Long-term Memory</h1>
  </div>

  {#if !$auth.user}
    <div class="bg-amber-50 border border-amber-200 rounded p-4 text-sm text-amber-800">
      Long-term memory requires authentication.
      <a href="/api/auth/login" class="underline ml-1">Log in</a>
    </div>
  {:else}
    <MemoryEditor />
  {/if}
</main>

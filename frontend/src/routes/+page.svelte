<!-- Document library / home page -->
<script>
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$api/client.js';
  import { auth } from '$stores/auth.js';

  let docs = [];
  let uploading = false;
  let error = null;

  async function deleteDoc(id, filename) {
    if (!confirm(`Delete "${filename}"? This cannot be undone.`)) return;
    try {
      await api.delete(`/pdfs/${id}`);
      docs = docs.filter(d => d.id !== id);
    } catch (e) {
      error = e.message;
    }
  }

  onMount(async () => {
    try {
      docs = await api.get('/pdfs');
    } catch (e) {
      error = e.message;
    }
  });

  async function handleUpload(event) {
    const file = event.target.files[0];
    if (!file) return;
    uploading = true;
    try {
      const form = new FormData();
      form.append('file', file);
      const meta = await api.upload('/pdfs', form);

      // Authenticated users: ask about long-term memory
      if ($auth.user?.authenticated) {
        const addToMemory = confirm(
          "Do you want to add this document's knowledge to your long-term memory?"
        );
        if (addToMemory) {
          try {
            await api.post('/memory/append', { doc_id: meta.id });
          } catch (memErr) {
            // Non-fatal: memory append failure should not block opening the document
            console.warn('Memory append failed:', memErr.message);
          }
        }
      }

      await goto(`/viewer/${meta.id}`);
    } catch (e) {
      error = e.message;
    } finally {
      uploading = false;
    }
  }
</script>

<main class="min-h-screen bg-gray-50 p-8">
  <header class="flex items-center justify-between mb-8 flex-wrap gap-3">
    <h1 class="text-2xl font-bold text-gray-900">NimbusPDF</h1>

    <nav class="flex items-center gap-4">
      {#if $auth.user?.authenticated}
        <a href="/memory" class="text-sm text-gray-600 hover:text-gray-900">Memory</a>
        <a href="/categories" class="text-sm text-gray-600 hover:text-gray-900">Categories</a>
        <a href="/settings" class="text-sm text-gray-600 hover:text-gray-900">Settings</a>
        <span class="text-sm text-gray-400">{$auth.user.email}</span>
        <a href="/api/auth/logout" class="text-sm text-blue-600 hover:text-blue-800">Logout</a>
      {:else}
        <a href="/settings" class="text-sm text-gray-600 hover:text-gray-900">Settings</a>
        <a href="/api/auth/login" class="text-sm text-blue-600 hover:text-blue-800">Login</a>
      {/if}
    </nav>
  </header>

  <label class="block w-full max-w-xs cursor-pointer bg-blue-600 text-white text-center py-3 rounded-lg hover:bg-blue-700">
    {uploading ? 'Uploading…' : '+ Open PDF'}
    <input type="file" accept="application/pdf" class="hidden" on:change={handleUpload} />
  </label>

  {#if error}
    <p class="text-red-600 mt-4">{error}</p>
  {/if}

  <section class="mt-8 grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-4">
    {#each docs as doc}
      <div class="relative group bg-white rounded-lg shadow hover:shadow-md transition">
        <a href="/viewer/{doc.id}" class="block p-4 pr-8">
          <p class="font-medium truncate">{doc.filename}</p>
          <p class="text-xs text-gray-400 mt-1">{doc.uploaded_at}</p>
        </a>
        <button
          on:click|preventDefault={() => deleteDoc(doc.id, doc.filename)}
          class="absolute top-2 right-2 w-6 h-6 rounded-full flex items-center justify-center text-xs text-gray-400 hover:bg-red-100 hover:text-red-600 opacity-0 group-hover:opacity-100 transition-opacity"
          title="Delete document"
        >✕</button>
      </div>
    {/each}
  </section>
</main>

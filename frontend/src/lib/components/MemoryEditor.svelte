<!--
  Markdown editor + live preview for long-term memory.
  - Left: textarea (markdown source)
  - Right: rendered HTML preview
  - Auto-saves on blur; explicit Save button
-->
<script>
  import { onMount } from 'svelte'
  import { marked } from 'marked'
  import { auth } from '$stores/auth.js'
  import { api } from '$api/client.js'

  let content = ''
  let saving = false
  let saved = false
  let error = null

  $: preview = marked(content)

  onMount(async () => {
    if (!$auth.user) return
    try {
      const data = await api.get('/memory')
      content = data.content ?? ''
    } catch (e) {
      error = e.message
    }
  })

  async function save() {
    if (saving) return
    saving = true
    error = null
    try {
      await api.put('/memory', { content })
      saved = true
      setTimeout(() => { saved = false }, 2000)
    } catch (e) {
      error = e.message
    } finally {
      saving = false
    }
  }

  function onBlur() {
    if ($auth.user) save()
  }
</script>

{#if !$auth.user}
  <div class="bg-amber-50 border border-amber-200 rounded p-4 text-sm text-amber-800">
    Long-term memory requires authentication.
    <a href="/api/auth/login" class="underline ml-1">Log in</a>
  </div>
{:else}
  <div class="flex flex-col gap-3">
    <div class="flex items-center justify-between">
      <p class="text-sm text-gray-500">Write markdown. Changes are saved automatically on blur.</p>
      <div class="flex items-center gap-2">
        {#if error}
          <span class="text-xs text-red-500">{error}</span>
        {/if}
        {#if saved}
          <span class="text-xs text-green-600">Saved</span>
        {/if}
        <button
          on:click={save}
          disabled={saving}
          class="bg-blue-600 text-white px-3 py-1.5 rounded text-sm hover:bg-blue-700 disabled:opacity-50"
        >
          {saving ? 'Saving…' : 'Save'}
        </button>
      </div>
    </div>

    <div class="grid grid-cols-2 gap-4" style="min-height: 400px;">
      <!-- Editor -->
      <div class="flex flex-col">
        <p class="text-xs font-medium text-gray-500 mb-1">Markdown</p>
        <textarea
          bind:value={content}
          on:blur={onBlur}
          class="flex-1 w-full border rounded px-3 py-2 text-sm font-mono resize-none focus:outline-none focus:ring-1 focus:ring-blue-400"
          placeholder="Write your long-term memory notes in Markdown…"
          style="min-height: 380px;"
        ></textarea>
      </div>

      <!-- Preview -->
      <div class="flex flex-col">
        <p class="text-xs font-medium text-gray-500 mb-1">Preview</p>
        <div
          class="flex-1 border rounded px-3 py-2 text-sm overflow-y-auto prose prose-sm max-w-none bg-gray-50"
          style="min-height: 380px;"
        >
          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
          {@html preview || '<p class="text-gray-400">Preview will appear here…</p>'}
        </div>
      </div>
    </div>
  </div>
{/if}

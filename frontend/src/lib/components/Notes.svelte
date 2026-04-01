<!--
  Per-page notes editor.
  - Auto-saves with debounce after typing
  - Shows "Saved" / "Saving…" status
  - Loads note content from pdfStore on mount/page change
-->
<script>
  import { pdfStore } from '$stores/pdf.js'

  export let docId
  export let page

  let content = ''
  let status = '' // '', 'saving', 'saved'
  let initialized = false

  // Sync content from store whenever the page prop changes
  $: {
    const stored = $pdfStore.notes[page]
    // Only update from store if we haven't made local edits since last page switch
    if (!initialized || page !== currentPage) {
      content = stored ?? ''
      initialized = true
      currentPage = page
    }
  }

  let currentPage = page

  function onInput() {
    status = 'saving'
    pdfStore.saveNote(docId, page, content)
    // Status update after the debounce window
    setTimeout(() => {
      if (status === 'saving') {
        status = 'saved'
        setTimeout(() => { if (status === 'saved') status = '' }, 2000)
      }
    }, 1200)
  }
</script>

<div class="p-3 flex flex-col gap-2">
  <textarea
    bind:value={content}
    on:input={onInput}
    placeholder="Notes for page {page}…"
    rows="5"
    class="w-full border rounded px-3 py-2 text-sm resize-none focus:outline-none focus:ring-1 focus:ring-blue-400"
  ></textarea>
  {#if status}
    <p class="text-xs {status === 'saved' ? 'text-green-600' : 'text-gray-400'}">
      {status === 'saving' ? 'Saving…' : 'Saved'}
    </p>
  {/if}
</div>

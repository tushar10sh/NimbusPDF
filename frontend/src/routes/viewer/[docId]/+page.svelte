<!-- PDF viewer page: toolbar + PDF canvas + AI sidebar + Notes drawer -->
<script>
  import { page } from '$app/stores';
  import PDFViewer from '$components/PDFViewer.svelte';
  import AISidebar from '$components/AISidebar.svelte';
  import Notes from '$components/Notes.svelte';
  import { pdfStore } from '$stores/pdf.js';

  const docId = $page.params.docId;
  let sidebarOpen = true;
  let notesOpen = false;

  $: pdfStore.load(docId);
</script>

<div class="flex flex-col h-screen overflow-hidden">
  <!-- Main row: viewer + sidebar -->
  <div class="flex flex-1 overflow-hidden">
    <!-- Main viewer area -->
    <div class="flex-1 flex flex-col overflow-hidden">
      <!-- Toolbar -->
      <div class="h-12 bg-white border-b flex items-center gap-3 px-4 shrink-0">
        <a href="/" class="text-sm text-gray-500 hover:text-gray-900">← Library</a>
        <span class="flex-1 truncate text-sm font-medium">{$pdfStore.filename ?? ''}</span>

        <!-- Zoom controls -->
        <button on:click={() => pdfStore.zoomOut()} class="px-2 py-1 text-sm border rounded hover:bg-gray-50">−</button>
        <span class="text-sm w-12 text-center">{$pdfStore.zoom}%</span>
        <button on:click={() => pdfStore.zoomIn()} class="px-2 py-1 text-sm border rounded hover:bg-gray-50">+</button>

        <!-- Search -->
        <input
          type="search"
          placeholder="Search…"
          class="border rounded px-2 py-1 text-sm w-48"
          on:input={(e) => pdfStore.search(e.target.value)}
        />

        <!-- Toggle Notes drawer -->
        <button
          on:click={() => (notesOpen = !notesOpen)}
          class="px-2 py-1 text-sm border rounded hover:bg-gray-50"
          class:bg-amber-50={notesOpen}
          class:border-amber-300={notesOpen}
        >
          {notesOpen ? 'Hide Notes' : 'Notes'}
        </button>

        <!-- Toggle AI sidebar -->
        <button
          on:click={() => (sidebarOpen = !sidebarOpen)}
          class="px-2 py-1 text-sm border rounded hover:bg-gray-50"
        >
          {sidebarOpen ? 'Hide AI' : 'AI Assistant'}
        </button>
      </div>

      <!-- PDF canvas -->
      <div class="flex-1 overflow-auto bg-gray-200">
        <PDFViewer {docId} />
      </div>
    </div>

    <!-- AI sidebar -->
    {#if sidebarOpen}
      <div class="w-96 border-l bg-white flex flex-col shrink-0">
        <AISidebar {docId} />
      </div>
    {/if}
  </div>

  <!-- Notes drawer at bottom -->
  <div class="border-t bg-white shrink-0" class:hidden={!notesOpen}>
    <div class="flex items-center px-4 py-2 border-b">
      <span class="text-sm font-medium">Notes — Page {$pdfStore.currentPage}</span>
      <button
        on:click={() => (notesOpen = false)}
        class="ml-auto text-xs text-gray-400 hover:text-gray-700"
      >
        Hide
      </button>
    </div>
    {#if notesOpen}
      <Notes {docId} page={$pdfStore.currentPage} />
    {/if}
  </div>
</div>

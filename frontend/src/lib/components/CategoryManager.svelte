<!--
  Document categorization UI.
  - Left: list of documents with category assignment dropdown
  - Right: list of categories with their documents
  - Add / remove categories
  - Persists to /api/categories/graph
-->
<script>
  import { onMount } from 'svelte'
  import { api } from '$api/client.js'

  let docs = []
  let graph = { nodes: [], edges: [] }
  let newCategoryName = ''
  let saving = false
  let error = null

  onMount(async () => {
    try {
      ;[docs, graph] = await Promise.all([
        api.get('/pdfs'),
        api.get('/categories/graph').catch(() => ({ nodes: [], edges: [] })),
      ])
      // Ensure nodes/edges always arrays
      graph.nodes = graph.nodes ?? []
      graph.edges = graph.edges ?? []
    } catch (e) {
      error = e.message
    }
  })

  function getCategories() {
    return graph.nodes.filter(n => n.kind === 'category')
  }

  function getDocCategory(docId) {
    const edge = graph.edges.find(e => e.source === docId && e.relation === 'belongs_to')
    return edge?.target ?? ''
  }

  function getDocsInCategory(categoryId) {
    const docIds = graph.edges
      .filter(e => e.target === categoryId && e.relation === 'belongs_to')
      .map(e => e.source)
    return docs.filter(d => docIds.includes(d.id))
  }

  async function persistGraph() {
    saving = true
    error = null
    try {
      await api.put('/categories/graph', graph)
    } catch (e) {
      error = e.message
    } finally {
      saving = false
    }
  }

  async function assignCategory(docId, categoryId) {
    // Remove existing belongs_to edge for this doc
    graph.edges = graph.edges.filter(e => !(e.source === docId && e.relation === 'belongs_to'))
    if (categoryId) {
      graph.edges = [...graph.edges, { source: docId, target: categoryId, relation: 'belongs_to' }]
    }
    graph = graph  // trigger reactivity
    await persistGraph()
  }

  async function addCategory() {
    const name = newCategoryName.trim()
    if (!name) return
    const id = crypto.randomUUID()
    graph.nodes = [...graph.nodes, { id, kind: 'category', label: name, doc_id: null }]
    newCategoryName = ''
    await persistGraph()
  }

  async function removeCategory(categoryId) {
    graph.nodes = graph.nodes.filter(n => n.id !== categoryId)
    graph.edges = graph.edges.filter(e => e.target !== categoryId)
    graph = graph  // trigger reactivity
    await persistGraph()
  }

  function handleAddKeydown(e) {
    if (e.key === 'Enter') addCategory()
  }
</script>

{#if error}
  <div class="bg-red-50 border border-red-200 rounded p-3 text-sm text-red-700 mb-4">{error}</div>
{/if}

<div class="grid grid-cols-2 gap-6">
  <!-- Left: Documents -->
  <div>
    <h2 class="text-base font-semibold mb-3">Documents</h2>
    {#if docs.length === 0}
      <p class="text-sm text-gray-400">No documents uploaded yet.</p>
    {:else}
      <div class="space-y-2">
        {#each docs as doc}
          <div class="flex items-center gap-3 bg-white border rounded p-3">
            <span class="flex-1 text-sm truncate" title={doc.filename}>{doc.filename}</span>
            <select
              class="text-xs border rounded px-2 py-1"
              value={getDocCategory(doc.id)}
              on:change={(e) => assignCategory(doc.id, e.target.value)}
              disabled={saving}
            >
              <option value="">— No category —</option>
              {#each getCategories() as cat}
                <option value={cat.id}>{cat.label}</option>
              {/each}
            </select>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Right: Categories -->
  <div>
    <h2 class="text-base font-semibold mb-3">Categories</h2>

    <!-- Add category -->
    <div class="flex gap-2 mb-4">
      <input
        bind:value={newCategoryName}
        on:keydown={handleAddKeydown}
        placeholder="New category name…"
        class="flex-1 border rounded px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400"
        disabled={saving}
      />
      <button
        on:click={addCategory}
        disabled={saving || !newCategoryName.trim()}
        class="bg-blue-600 text-white px-3 py-1.5 rounded text-sm hover:bg-blue-700 disabled:opacity-50"
      >
        Add
      </button>
    </div>

    {#if getCategories().length === 0}
      <p class="text-sm text-gray-400">No categories yet. Add one above.</p>
    {:else}
      <div class="space-y-3">
        {#each getCategories() as cat}
          <div class="bg-white border rounded p-3">
            <div class="flex items-center justify-between mb-2">
              <span class="text-sm font-medium">{cat.label}</span>
              <button
                on:click={() => removeCategory(cat.id)}
                class="text-xs text-red-500 hover:text-red-700 disabled:opacity-50"
                disabled={saving}
              >
                Remove
              </button>
            </div>
            {#if getDocsInCategory(cat.id).length === 0}
              <p class="text-xs text-gray-400">No documents assigned.</p>
            {:else}
              <ul class="space-y-1">
                {#each getDocsInCategory(cat.id) as doc}
                  <li class="text-xs text-gray-600 truncate">• {doc.filename}</li>
                {/each}
              </ul>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if saving}
      <p class="text-xs text-gray-400 mt-2">Saving…</p>
    {/if}
  </div>
</div>

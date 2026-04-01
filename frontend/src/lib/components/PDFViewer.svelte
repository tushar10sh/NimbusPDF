<!--
  Full PDF viewer using pdfjs-dist.
  - One canvas per page, stacked vertically
  - Highlight overlay: colored divs over canvas using PDF coordinate system
  - Text selection: mouseup → color picker popover → save highlight
  - IntersectionObserver tracks current page
  - Re-renders on zoom change with device-pixel-ratio scaling for crisp text on HiDPI displays
  - Keyword search: highlights matching text items across all pages using PDF.js text content API
-->
<script>
  import { onMount, onDestroy, tick } from 'svelte'
  import * as pdfjsLib from 'pdfjs-dist'
  import { pdfStore } from '$stores/pdf.js'

  export let docId

  // Use the worker copied by vite-plugin-static-copy at build time
  pdfjsLib.GlobalWorkerOptions.workerSrc = '/pdf.worker.min.js'

  const HIGHLIGHT_COLORS = {
    yellow: 'rgba(255, 235, 59, 0.45)',
    green:  'rgba(76, 175, 80, 0.35)',
    blue:   'rgba(33, 150, 243, 0.35)',
    pink:   'rgba(233, 30, 99, 0.35)',
  }

  let container
  let pdfDoc = null
  let pageCount = 0
  let observer
  let renderVersion = 0

  // Color picker state
  let pickerVisible = false
  let pickerX = 0
  let pickerY = 0
  let pendingSelection = null  // { text, page, rects }

  // Map pageNum → { canvas, overlayEl, viewport }
  let pageRefs = {}

  // Search: rawTextCache stores PDF-space items (zoom-invariant); searchResults stores CSS-px rects
  let rawTextCache = {}   // pageNum → PDF.js text items
  let searchResults = {}  // pageNum → [{ left, top, width, height }]
  let lastSearchQuery = ''

  onMount(async () => {
    const url = `/api/pdfs/${docId}/file`
    pdfDoc = await pdfjsLib.getDocument(url).promise
    pageCount = pdfDoc.numPages
    pdfStore.setPageCount(pageCount)

    observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            pdfStore.setCurrentPage(Number(e.target.dataset.page))
          }
        }
      },
      { root: container, threshold: 0.3 }
    )

    await tick()
    for (let i = 1; i <= pageCount; i++) {
      await renderPageNum(i)
    }
  })

  onDestroy(() => observer?.disconnect())

  // Re-render all pages when zoom changes
  let lastZoom = null
  $: if (pdfDoc && $pdfStore.zoom !== lastZoom) {
    lastZoom = $pdfStore.zoom
    rerenderAll()
  }

  // Re-run search when query changes (zoom changes go through rerenderAll which calls runSearch)
  $: if (pdfDoc) handleSearch($pdfStore.searchQuery)

  async function rerenderAll() {
    if (!pdfDoc) return
    renderVersion++
    const myVersion = renderVersion
    for (let i = 1; i <= pageCount; i++) {
      if (renderVersion !== myVersion) break
      await renderPageNum(i)
    }
    // Refresh search rect positions now that viewports are updated
    if (lastSearchQuery) await runSearch(lastSearchQuery)
  }

  async function renderPageNum(pageNum) {
    const ref = pageRefs[pageNum]
    if (!ref || !pdfDoc) return

    const pg = await pdfDoc.getPage(pageNum)
    const scale = $pdfStore.zoom / 100
    const viewport = pg.getViewport({ scale })
    const dpr = window.devicePixelRatio || 1

    const canvas = ref.canvas
    const ctx = canvas.getContext('2d')

    // Physical pixels — scaled by DPR for crisp rendering on HiDPI / Retina displays
    canvas.width = Math.floor(viewport.width * dpr)
    canvas.height = Math.floor(viewport.height * dpr)

    // CSS display size stays at logical viewport dimensions
    canvas.style.width = viewport.width + 'px'
    canvas.style.height = viewport.height + 'px'

    ref.wrapper.style.width = viewport.width + 'px'
    ref.wrapper.style.height = viewport.height + 'px'
    ref.overlay.style.width = viewport.width + 'px'
    ref.overlay.style.height = viewport.height + 'px'

    // Scale context so PDF.js renders at full physical resolution
    ctx.scale(dpr, dpr)
    await pg.render({ canvasContext: ctx, viewport }).promise

    pageRefs[pageNum].viewport = viewport
    observer?.observe(ref.wrapper)
  }

  // ── Search ────────────────────────────────────────────────────────────────

  async function handleSearch(query) {
    if (!query?.trim()) {
      searchResults = {}
      lastSearchQuery = ''
      return
    }
    lastSearchQuery = query
    await runSearch(query)
  }

  async function runSearch(query) {
    if (!pdfDoc) return
    const q = query.toLowerCase().trim()
    const result = {}
    let firstMatchPage = null

    for (let pageNum = 1; pageNum <= pageCount; pageNum++) {
      // Fetch and cache raw text items in PDF coordinate space (zoom-invariant)
      if (!rawTextCache[pageNum]) {
        try {
          const pg = await pdfDoc.getPage(pageNum)
          const content = await pg.getTextContent()
          rawTextCache[pageNum] = content.items
        } catch {
          continue
        }
      }

      // Abort if a newer query has been issued
      if (lastSearchQuery !== query) return

      const viewport = pageRefs[pageNum]?.viewport
      if (!viewport) continue

      const rects = []
      for (const item of rawTextCache[pageNum]) {
        if (!item.str?.toLowerCase().includes(q)) continue
        // transform = [scaleX, skewY, skewX, scaleY, tx, ty] — tx/ty are PDF-space origin
        const [, , , , x, y] = item.transform
        const w = item.width
        const h = item.height
        // convertToViewportRectangle handles the PDF→canvas coordinate flip
        const [vx1, vy1, vx2, vy2] = viewport.convertToViewportRectangle([x, y, x + w, y + h])
        rects.push({
          left:   Math.min(vx1, vx2),
          top:    Math.min(vy1, vy2),
          width:  Math.abs(vx2 - vx1),
          height: Math.abs(vy2 - vy1),
        })
      }
      if (rects.length) {
        result[pageNum] = rects
        if (!firstMatchPage) firstMatchPage = pageNum
      }
    }

    if (lastSearchQuery !== query) return
    searchResults = result

    if (firstMatchPage && pageRefs[firstMatchPage]) {
      await tick()
      pageRefs[firstMatchPage].wrapper.scrollIntoView({ behavior: 'smooth', block: 'start' })
    }
  }

  // ── Highlights ───────────────────────────────────────────────────────────

  function onMouseUp(event) {
    const selection = window.getSelection()
    if (!selection || selection.isCollapsed) {
      dismissPicker()
      return
    }

    const selectedText = selection.toString().trim()
    if (!selectedText) {
      dismissPicker()
      return
    }

    const anchorNode = selection.anchorNode
    const pageEl = anchorNode?.parentElement?.closest('[data-page]')
    if (!pageEl) {
      dismissPicker()
      return
    }

    const pageNum = Number(pageEl.dataset.page)
    const range = selection.getRangeAt(0)
    const rects = Array.from(range.getClientRects())

    const wrapperRect = pageEl.getBoundingClientRect()
    const relativeRects = rects.map(r => ({
      left: r.left - wrapperRect.left,
      top: r.top - wrapperRect.top,
      width: r.width,
      height: r.height,
    }))

    pendingSelection = { text: selectedText, page: pageNum, rects: relativeRects }

    const lastRect = rects[rects.length - 1]
    pickerX = lastRect.right
    pickerY = lastRect.bottom + window.scrollY + 4
    pickerVisible = true
  }

  function dismissPicker() {
    pickerVisible = false
    pendingSelection = null
  }

  async function applyHighlight(colorKey) {
    if (!pendingSelection) return
    const { text, page, rects } = pendingSelection

    const newHighlight = {
      id: crypto.randomUUID(),
      page,
      text,
      color: colorKey,
      rects,
      created_at: new Date().toISOString(),
    }

    const updated = [...$pdfStore.highlights, newHighlight]
    await pdfStore.saveHighlights(updated)

    window.getSelection()?.removeAllRanges()
    dismissPicker()
  }

  async function removeHighlight(id) {
    const updated = $pdfStore.highlights.filter(h => h.id !== id)
    await pdfStore.saveHighlights(updated)
  }

  function highlightsForPage(pageNum) {
    return ($pdfStore.highlights ?? []).filter(h => h.page === pageNum)
  }

  function registerPage(node, pageNum) {
    const canvas = node.querySelector('canvas')
    const overlay = node.querySelector('.highlight-overlay')
    pageRefs[pageNum] = { wrapper: node, canvas, overlay, viewport: null }

    return {
      destroy() {
        delete pageRefs[pageNum]
      }
    }
  }
</script>

<svelte:window on:mouseup={onMouseUp} />

<!-- Color picker popover -->
{#if pickerVisible}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div
    class="fixed z-50 bg-white border border-gray-200 rounded-lg shadow-lg p-2 flex gap-1"
    style="left:{pickerX}px; top:{pickerY}px; transform: translateX(-50%);"
    on:click|stopPropagation
  >
    {#each Object.entries(HIGHLIGHT_COLORS) as [colorKey, colorVal]}
      <button
        class="w-7 h-7 rounded-full border-2 border-white hover:scale-110 transition-transform"
        style="background:{colorVal}; border-color: #ccc;"
        title={colorKey}
        on:click={() => applyHighlight(colorKey)}
      ></button>
    {/each}
    <button
      class="w-7 h-7 rounded flex items-center justify-center text-gray-400 hover:text-gray-700 text-xs ml-1"
      title="Cancel"
      on:click={dismissPicker}
    >✕</button>
  </div>
{/if}

<!-- Click outside to dismiss picker -->
{#if pickerVisible}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div class="fixed inset-0 z-40" on:click={dismissPicker}></div>
{/if}

<div bind:this={container} class="flex flex-col items-center gap-4 py-4">
  {#each Array.from({ length: pageCount }, (_, i) => i + 1) as pageNum}
    <div
      data-page={pageNum}
      class="relative bg-white shadow select-text"
      use:registerPage={pageNum}
    >
      <canvas></canvas>

      <!-- Saved highlight overlay -->
      <div class="highlight-overlay absolute inset-0 pointer-events-none">
        {#each highlightsForPage(pageNum) as hl (hl.id)}
          {#each hl.rects as rect}
            <div
              class="absolute pointer-events-auto cursor-pointer group"
              style="
                left:{rect.left}px;
                top:{rect.top}px;
                width:{rect.width}px;
                height:{rect.height}px;
                background:{HIGHLIGHT_COLORS[hl.color] ?? HIGHLIGHT_COLORS.yellow};
                border-radius:2px;
              "
              title="Click to remove highlight"
              on:click|stopPropagation={() => removeHighlight(hl.id)}
              role="button"
              tabindex="0"
              on:keydown={(e) => e.key === 'Enter' && removeHighlight(hl.id)}
            ></div>
          {/each}
        {/each}
      </div>

      <!-- Search results overlay -->
      {#if searchResults[pageNum]?.length}
        <div class="absolute inset-0 pointer-events-none" style="z-index:5">
          {#each searchResults[pageNum] as rect}
            <div
              class="absolute"
              style="
                left:{rect.left}px;
                top:{rect.top}px;
                width:{rect.width}px;
                height:{rect.height}px;
                background:rgba(255,200,0,0.45);
                border:1px solid rgba(200,140,0,0.7);
                border-radius:1px;
              "
            ></div>
          {/each}
        </div>
      {/if}
    </div>
  {/each}
</div>

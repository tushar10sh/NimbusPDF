import { writable, get } from 'svelte/store'
import { api } from '$api/client.js'

const DEFAULT = {
  docId: null,
  filename: null,
  pageCount: 0,
  currentPage: 1,
  zoom: 100,
  searchQuery: '',
  highlights: [],
  notes: {},
}

function createPdfStore() {
  const { subscribe, update, set } = writable({ ...DEFAULT })

  let noteSaveTimers = {}

  const store = {
    subscribe,

    async load(docId) {
      set({ ...DEFAULT, docId })
      const [meta, highlights, notes] = await Promise.all([
        api.get(`/pdfs/${docId}`),
        api.get(`/pdfs/${docId}/highlights`),
        api.get(`/pdfs/${docId}/notes`),
      ])
      update(s => ({
        ...s,
        filename: meta.filename,
        highlights: highlights ?? [],
        notes: Object.fromEntries((notes ?? []).map(n => [n.page, n.content])),
      }))
    },

    setPageCount(n) { update(s => ({ ...s, pageCount: n })) },
    setCurrentPage(n) { update(s => ({ ...s, currentPage: n })) },
    zoomIn() { update(s => ({ ...s, zoom: Math.min(s.zoom + 10, 300) })) },
    zoomOut() { update(s => ({ ...s, zoom: Math.max(s.zoom - 10, 30) })) },
    search(query) { update(s => ({ ...s, searchQuery: query })) },

    async saveHighlights(highlights) {
      const { docId } = get(store)
      update(s => ({ ...s, highlights }))
      if (docId) await api.put(`/pdfs/${docId}/highlights`, highlights)
    },

    clearHighlights() {
      update(s => ({ ...s, highlights: [] }))
    },

    updateNote(page, content) {
      update(s => ({ ...s, notes: { ...s.notes, [page]: content } }))
    },

    saveNote(docId, page, content) {
      update(s => ({ ...s, notes: { ...s.notes, [page]: content } }))
      clearTimeout(noteSaveTimers[page])
      noteSaveTimers[page] = setTimeout(async () => {
        await api.put(`/pdfs/${docId}/notes/${page}`, {
          content,
          page,
          updated_at: new Date().toISOString(),
        }).catch(console.error)
      }, 800)
    },
  }

  return store
}

export const pdfStore = createPdfStore()

<!--
  AI chat sidebar.
  - Loads chat history from backend on mount
  - Sends messages to /api/ai/chat
  - Quick-action buttons: Summary, Key Points
  - Streaming indicator with animated dots
  - Clear chat button
-->
<script>
  import { onMount } from 'svelte'
  import { api } from '$api/client.js'

  export let docId

  let messages = []
  let input = ''
  let loading = false
  let chatEl

  onMount(async () => {
    try {
      const history = await api.get(`/ai/history/${docId}`)
      if (Array.isArray(history)) {
        messages = history
      }
    } catch {
      // history unavailable — start fresh
    }
  })

  async function send() {
    if (!input.trim() || loading) return
    const userMsg = input.trim()
    input = ''
    messages = [...messages, { role: 'user', content: userMsg }]
    loading = true
    try {
      const { reply } = await api.post('/ai/chat', { doc_id: docId, message: userMsg })
      messages = [...messages, { role: 'assistant', content: reply }]
    } catch (e) {
      messages = [...messages, { role: 'assistant', content: `Error: ${e.message}` }]
    } finally {
      loading = false
      setTimeout(() => chatEl?.scrollTo({ top: chatEl.scrollHeight, behavior: 'smooth' }), 50)
    }
  }

  async function quickAction(action) {
    loading = true
    try {
      const endpoint = action === 'summary' ? '/ai/summary' : '/ai/keypoints'
      const { reply } = await api.post(endpoint, { doc_id: docId })
      messages = [...messages, { role: 'assistant', content: reply }]
    } catch (e) {
      messages = [...messages, { role: 'assistant', content: `Error: ${e.message}` }]
    } finally {
      loading = false
      setTimeout(() => chatEl?.scrollTo({ top: chatEl.scrollHeight, behavior: 'smooth' }), 50)
    }
  }

  function clearChat() {
    messages = []
  }
</script>

<div class="flex flex-col h-full">
  <!-- Action buttons row -->
  <div class="p-3 border-b flex gap-2 shrink-0 flex-wrap">
    <button
      on:click={() => quickAction('summary')}
      class="flex-1 text-xs border rounded px-2 py-1 hover:bg-gray-50 disabled:opacity-50"
      disabled={loading}
    >
      Summary
    </button>
    <button
      on:click={() => quickAction('keypoints')}
      class="flex-1 text-xs border rounded px-2 py-1 hover:bg-gray-50 disabled:opacity-50"
      disabled={loading}
    >
      Key Points
    </button>
    <button
      on:click={clearChat}
      class="text-xs border rounded px-2 py-1 hover:bg-red-50 hover:border-red-200 hover:text-red-600 disabled:opacity-50"
      disabled={loading}
      title="Clear chat history"
    >
      Clear
    </button>
  </div>

  <!-- Chat history -->
  <div bind:this={chatEl} class="flex-1 overflow-y-auto p-3 space-y-3">
    {#each messages as msg}
      <div class={msg.role === 'user' ? 'text-right' : 'text-left'}>
        <span
          class={`inline-block max-w-xs px-3 py-2 rounded-lg text-sm whitespace-pre-wrap break-words ${
            msg.role === 'user'
              ? 'bg-blue-600 text-white'
              : 'bg-gray-100 text-gray-900'
          }`}
        >
          {msg.content}
        </span>
      </div>
    {/each}

    {#if loading}
      <div class="text-left">
        <span class="inline-flex items-center gap-1 bg-gray-100 px-3 py-2 rounded-lg">
          <span class="w-1.5 h-1.5 bg-gray-400 rounded-full animate-bounce" style="animation-delay:0ms"></span>
          <span class="w-1.5 h-1.5 bg-gray-400 rounded-full animate-bounce" style="animation-delay:150ms"></span>
          <span class="w-1.5 h-1.5 bg-gray-400 rounded-full animate-bounce" style="animation-delay:300ms"></span>
        </span>
      </div>
    {/if}

    {#if messages.length === 0 && !loading}
      <p class="text-xs text-gray-400 text-center pt-4">Ask anything about this document.</p>
    {/if}
  </div>

  <!-- Input -->
  <form on:submit|preventDefault={send} class="p-3 border-t flex gap-2 shrink-0">
    <input
      bind:value={input}
      placeholder="Ask about this document…"
      class="flex-1 border rounded px-3 py-2 text-sm disabled:bg-gray-50"
      disabled={loading}
    />
    <button
      type="submit"
      class="bg-blue-600 text-white px-3 py-2 rounded text-sm hover:bg-blue-700 disabled:opacity-50"
      disabled={loading}
    >
      Send
    </button>
  </form>
</div>

<script>
  import '../app.css';
  import { onMount } from 'svelte';
  import { auth } from '$stores/auth.js';

  onMount(async () => {
    await auth.init();
  });
</script>

<slot />

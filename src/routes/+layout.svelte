<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import './layout.css';
  import '@fontsource-variable/inter';
  import { Toaster } from "$lib/components/ui/sonner";

  let { children } = $props();

  onMount(() => {
    const timer = setTimeout(async () => {
      try {
        await invoke('show_window');
      } catch (err) {
        console.error(err);
      }
    }, 150);

    return () => clearTimeout(timer);
  });
  
</script>

<Toaster position="bottom-right" />
{@render children()}

<script lang="ts">
  import './layout.css';
  import '@fontsource-variable/inter';
  import { Toaster } from "$lib/components/ui/sonner";

  let { children } = $props();
</script>

<Toaster position="bottom-right" />
{@render children()}

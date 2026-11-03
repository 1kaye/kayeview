<script lang="ts">
	import { Toaster as Sonner, type ToasterProps as SonnerProps } from "svelte-sonner";
	import Loader2Icon from '@lucide/svelte/icons/loader-2';
	import CircleCheckIcon from '@lucide/svelte/icons/circle-check';
	import OctagonXIcon from '@lucide/svelte/icons/octagon-x';
	import InfoIcon from '@lucide/svelte/icons/info';
	import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';

	let { ...restProps }: SonnerProps = $props();
</script>

<Sonner
	theme="dark"
	class="toaster group"
	toastOptions={{
		classes: {
			toast: "group toast group-[.toaster]:bg-card/95 group-[.toaster]:backdrop-blur-md group-[.toaster]:text-card-foreground group-[.toaster]:border group-[.toaster]:border-border/80 group-[.toaster]:shadow-xl group-[.toaster]:rounded-xl group-[.toaster]:py-2.5 group-[.toaster]:px-3.5 group-[.toaster]:text-xs group-[.toaster]:gap-2.5 group-[.toaster]:select-none",
			description: "group-[.toast]:text-muted-foreground group-[.toast]:text-xs",
			actionButton: "group-[.toast]:bg-primary group-[.toast]:text-primary-foreground group-[.toast]:text-xs group-[.toast]:rounded-md group-[.toast]:px-2 group-[.toast]:py-1",
			cancelButton: "group-[.toast]:bg-muted group-[.toast]:text-muted-foreground group-[.toast]:text-xs group-[.toast]:rounded-md group-[.toast]:px-2 group-[.toast]:py-1",
			title: "group-[.toast]:text-xs group-[.toast]:font-medium",
		},
	}}
	{...restProps}
>
	{#snippet loadingIcon()}
		<Loader2Icon class="size-4 animate-spin text-primary" />
	{/snippet}
	{#snippet successIcon()}
		<CircleCheckIcon class="size-4 text-emerald-400" />
	{/snippet}
	{#snippet errorIcon()}
		<OctagonXIcon class="size-4 text-destructive" />
	{/snippet}
	{#snippet infoIcon()}
		<InfoIcon class="size-4 text-sky-400" />
	{/snippet}
	{#snippet warningIcon()}
		<TriangleAlertIcon class="size-4 text-amber-400" />
	{/snippet}
</Sonner>

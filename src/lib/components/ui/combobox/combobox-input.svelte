<script lang="ts">
	import { Combobox as ComboboxPrimitive } from "bits-ui";
	import { cn, type WithoutChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: WithoutChild<ComboboxPrimitive.InputProps> = $props();
</script>

<ComboboxPrimitive.Input
	bind:ref
	data-slot="combobox-input"
	class={cn(
		"dark:bg-input/30 border-input data-placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-ring/50 h-8 rounded-md border bg-transparent px-2.5 py-1 text-sm shadow-xs transition-[color,box-shadow] placeholder:text-muted-foreground w-full min-w-0 outline-none focus-visible:ring-3 disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50",
		className
	)}
	{...restProps}
/>

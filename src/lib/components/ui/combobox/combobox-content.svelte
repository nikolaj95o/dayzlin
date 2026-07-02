<script lang="ts">
	import { Combobox as ComboboxPrimitive } from "bits-ui";
	import { cn, type WithoutChild } from "$lib/utils.js";
	import type { ComponentProps } from "svelte";
	import type { WithoutChildrenOrChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		sideOffset = 4,
		portalProps,
		children,
		...restProps
	}: WithoutChild<ComboboxPrimitive.ContentProps> & {
		portalProps?: WithoutChildrenOrChild<ComponentProps<typeof ComboboxPrimitive.Portal>>;
	} = $props();
</script>

<ComboboxPrimitive.Portal {...portalProps}>
	<ComboboxPrimitive.Content
		bind:ref
		{sideOffset}
		data-slot="combobox-content"
		class={cn(
			"bg-popover text-popover-foreground data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0 data-closed:zoom-out-95 data-open:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=top]:slide-in-from-bottom-2 ring-foreground/10 min-w-36 rounded-md shadow-md ring-1 duration-100 relative isolate z-50 max-h-60 overflow-x-hidden overflow-y-auto p-1",
			className
		)}
		{...restProps}
	>
		<ComboboxPrimitive.Viewport class="w-full min-w-(--bits-combobox-anchor-width)">
			{@render children?.()}
		</ComboboxPrimitive.Viewport>
	</ComboboxPrimitive.Content>
</ComboboxPrimitive.Portal>

<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  // @tauri-apps/api doesn't export the ResizeDirection type, so mirror it here (matches the string
  // union accepted by Window.startResizeDragging).
  type ResizeDirection =
    | "North"
    | "South"
    | "East"
    | "West"
    | "NorthEast"
    | "NorthWest"
    | "SouthEast"
    | "SouthWest";

  // Native decorations are disabled (tauri.conf.json), so the OS provides no edge resize
  // cursors/hit-testing — especially on Linux/WebKitGTK under Wayland. We overlay thin invisible
  // grips around the window that show the right resize cursor and start a native resize drag.
  const appWindow = getCurrentWindow();
  let isMaximized = $state(false);

  onMount(() => {
    // Resizing a maximized window is meaningless, so hide the grips while maximized. Same sync
    // pattern as TitleBar.svelte.
    appWindow.isMaximized().then((m) => (isMaximized = m));
    const unResize = appWindow.onResized(() => {
      appWindow.isMaximized().then((m) => (isMaximized = m));
    });
    return () => unResize.then((f) => f());
  });

  function resize(direction: ResizeDirection) {
    return (e: PointerEvent) => {
      if (e.button !== 0) return; // left button only
      e.preventDefault();
      appWindow.startResizeDragging(direction as Parameters<typeof appWindow.startResizeDragging>[0]);
    };
  }

  // Edge thickness / corner size in px.
  const EDGE = 5;
  const CORNER = 11;
</script>

{#if !isMaximized}
  <!-- Overlay itself ignores pointer events; only the grips capture them. Kept below dialog
       overlays (z-40) so open dialogs win the corners. -->
  <div class="pointer-events-none fixed inset-0 z-40">
    <!-- Edges -->
    <div
      role="presentation"
      class="pointer-events-auto absolute inset-x-0 top-0"
      style="height:{EDGE}px;cursor:n-resize"
      onpointerdown={resize("North")}
    ></div>
    <div
      role="presentation"
      class="pointer-events-auto absolute inset-x-0 bottom-0"
      style="height:{EDGE}px;cursor:s-resize"
      onpointerdown={resize("South")}
    ></div>
    <div
      role="presentation"
      class="pointer-events-auto absolute inset-y-0 left-0"
      style="width:{EDGE}px;cursor:w-resize"
      onpointerdown={resize("West")}
    ></div>
    <div
      role="presentation"
      class="pointer-events-auto absolute inset-y-0 right-0"
      style="width:{EDGE}px;cursor:e-resize"
      onpointerdown={resize("East")}
    ></div>

    <!-- Corners sit on top of the edges so they win the diagonal cursor. -->
    <div
      role="presentation"
      class="pointer-events-auto absolute top-0 left-0"
      style="width:{CORNER}px;height:{CORNER}px;cursor:nw-resize"
      onpointerdown={resize("NorthWest")}
    ></div>
    <div
      role="presentation"
      class="pointer-events-auto absolute top-0 right-0"
      style="width:{CORNER}px;height:{CORNER}px;cursor:ne-resize"
      onpointerdown={resize("NorthEast")}
    ></div>
    <div
      role="presentation"
      class="pointer-events-auto absolute bottom-0 left-0"
      style="width:{CORNER}px;height:{CORNER}px;cursor:sw-resize"
      onpointerdown={resize("SouthWest")}
    ></div>
    <div
      role="presentation"
      class="pointer-events-auto absolute right-0 bottom-0"
      style="width:{CORNER}px;height:{CORNER}px;cursor:se-resize"
      onpointerdown={resize("SouthEast")}
    ></div>
  </div>
{/if}

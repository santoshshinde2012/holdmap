<script lang="ts">
  // Small inline icon set (Lucide-style strokes) so the app has no runtime icon dependency.
  let { name, size = 16, label }: { name: string; size?: number; label?: string } = $props();

  const paths: Record<string, string> = {
    star: "m12 2 3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2Z",
    graph: "M5 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM19 6a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM12 22a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM6.5 5.5l4.5 12.5M17.5 5.5 13 18M7 4h10",
    list: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
    layers: "m12 2 10 5-10 5L2 7l10-5ZM2 17l10 5 10-5M2 12l10 5 10-5",
    play: "M6 4l14 8-14 8V4Z",
    history: "M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l3 3",
    fit: "M3 9V3h6M21 9V3h-6M3 15v6h6M21 15v6h-6",
    bell: "M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9M13.73 21a2 2 0 0 1-3.46 0",
    search: "M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Zm10 2-4.35-4.35",
    refresh: "M21 12a9 9 0 1 1-2.64-6.36M21 3v6h-6",
    stop: "M7 7h10v10H7z",
    zap: "M13 2 3 14h9l-1 8 10-12h-9l1-8Z",
    external: "M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",
    copy: "M9 9h11v11H9zM5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1",
    check: "M20 6 9 17l-5-5",
    x: "M18 6 6 18M6 6l12 12",
    sun: "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10ZM12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42",
    "sun-moon": "M12 8a2.83 2.83 0 0 0 4 4 4 4 0 1 1-4-4M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M6.3 17.7l-1.4 1.4M19.1 4.9l-1.4 1.4",
    moon: "M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79Z",
    monitor: "M3 4h18v12H3zM8 20h8M12 16v4",
    globe: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z",
    lock: "M5 11h14v10H5zM8 11V7a4 4 0 0 1 8 0v4",
    shield: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z",
    alert: "M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0ZM12 9v4M12 17h.01",
    info: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM12 16v-4M12 8h.01",
    branch: "M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM18 9a9 9 0 0 1-9 9",
    box: "M21 16V8l-9-5-9 5v8l9 5 9-5ZM3.3 7 12 12l8.7-5M12 22V12",
    terminal: "m4 17 6-6-6-6M12 19h8",
    folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z",
    keyboard: "M2 6h20v12H2zM6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10",
    plug: "M9 2v6M15 2v6M6 8h12v4a6 6 0 0 1-12 0V8ZM12 18v4",
    sparkles: "M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9L12 3Z",
    radar: "M12 12m-1 0a1 1 0 1 0 2 0 1 1 0 1 0-2 0M19.07 4.93A10 10 0 1 0 22 12M16.24 7.76A6 6 0 1 0 18 12",
    help: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3M12 17h.01",
    filter: "M22 3H2l8 9.46V19l4 2v-8.54L22 3Z",
    command: "M18 3a3 3 0 0 0-3 3v12a3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3H6a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3 3 3 0 0 0 3 3h12a3 3 0 0 0 3-3 3 3 0 0 0-3-3Z",
    chevron: "m9 18 6-6-6-6",
    user: "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2M12 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z",
    clock: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM12 6v6l4 2",
    undo: "M3 7v6h6M3 13a9 9 0 1 0 3-7.7L3 8",
    tree: "M6 3v6M6 9a3 3 0 1 0 0 .01M6 12v3a3 3 0 0 0 3 3h6M18 18m-3 0a3 3 0 1 0 6 0 3 3 0 1 0-6 0",
    arrow: "M5 12h14M13 6l6 6-6 6",
    hash: "M4 9h16M4 15h16M10 3 8 21M16 3l-2 18",
    sliders: "M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6",
    server: "M3 4h18v7H3zM3 13h18v7H3zM7 7.5h.01M7 16.5h.01",
    "chevron-down": "m6 9 6 6 6-6",
    "chevron-up": "m18 15-6-6-6 6",
    "chevrons-ud": "m7 15 5 5 5-5M7 9l5-5 5 5",
    plus: "M12 5v14M5 12h14",
    minus: "M5 12h14",
    pencil: "M17 3a2.85 2.85 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z",
    activity: "M22 12h-4l-3 9L9 3l-3 9H2",
    cpu: "M5 5h14v14H5zM9 9h6v6H9zM9 1v4M15 1v4M9 19v4M15 19v4M1 9h4M1 15h4M19 9h4M19 15h4",
    link: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
    "arrow-left": "M19 12H5M11 18l-6-6 6-6",
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}
  style="flex: none"
>
  <path d={paths[name] ?? ""} />
</svg>

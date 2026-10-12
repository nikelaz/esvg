# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build (debug)
cargo build

# Build (release)
cargo build --release

# Run
cargo run

# Run (release)
cargo run --release
```

There are no tests currently.

## Architecture

This is a desktop GUI app built with **Slint** (UI framework) and **Rust** (backend logic). It provides a visual interface for optimizing SVG files using `esvg-rs` (the sibling `crates/core` crate in this workspace).

### Key layers

- **`ui/*.slint`** — Slint UI definitions. `build.rs` compiles these into Rust via `slint_build::compile("ui/main-window.slint")`. All components are imported transitively from `main-window.slint`.
- **`ui/app-state.slint`** — The single global state singleton (`AppState`) shared across all UI components and the Rust backend. All plugin toggles, export options, image buffers, and loading state live here.
- **`src/main.rs`** — Entire Rust backend. Sets up the Slint window, wires callbacks (`on_open_file`, `on_request_optimize`, `on_export_file`), and drives background threads for SVG loading, optimization, and rendering.
- **`crates/core`** — The SVG optimization library (`esvg-rs`, a workspace member). Exposes `optimize()`, `PluginConfig`, `apply_svg_export_options()`, and `SvgExportOptions`.

### Data flow

1. User opens a file → `on_open_file` callback → `load_svg()` runs on a background thread
2. `load_svg` parses the original SVG, renders its overview, optimizes with default `PluginConfig`, renders the optimized overview, and hands both trees to the `ViewportRenderer` from the UI thread
3. When plugin toggles change in the sidebar → `AppState.request_optimize()` callback → `render_optimized()` re-runs optimization+rendering with updated `PluginConfig` built from current `AppState`
4. Export → `AppState.export_file(format)` → saves optimized SVG or renders to PNG at user-specified dimensions

### Rendering pipeline

SVGs are rendered with `resvg` (`tiny_skia`); Slint's renderer (Skia, OpenGL on Windows) only composites the resulting bitmaps. The viewer never renders the whole document at a fixed high resolution:

- **Overview** — the whole document with its longest side at 2048px (`render_overview`). Shown as soon as it's ready and wherever no sharp frame covers the SVG.
- **Frame** — `src/viewport.rs`. The viewer calls `AppState.request-view-render(zoom, pan, viewport size)` when the view settles (60ms debounce while zooming/resizing), when a pan uncovers an edge, and when a pan ends. A single worker thread renders only the visible area plus a 25% overscan margin at device resolution, snapped to whole physical pixels so it is shown 1:1. It always renders the latest request, renders the variant on screen first, and then the other variant so the original/optimized toggle is instant. Until a new frame arrives the old one is translated (pan) or scaled (zoom).
- `ViewFrame` geometry is in SVG user units, so it stays correct under any later pan/zoom. The overview is drawn as four clipped strips around the frame so the two never overlap.
- Zoom ranges from 5% of fit to 10× (1000%). `zoom_enabled` turns on once the overview is ready.

### Cancellation

`render_optimized` uses an `Arc<AtomicU64>` generation counter to cancel stale in-flight renders when the user changes plugin settings rapidly. `ViewportRenderer` keeps a per-variant generation that `set_tree`/`clear` bump, so frames of a replaced tree are dropped. Call those on the UI thread so a stale frame can't land after a reset.

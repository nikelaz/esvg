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

This is a desktop GUI app built with **Slint** (UI framework) and **Rust** (backend logic). It provides a visual interface for optimizing SVG files using `esvg-rs` (a git submodule at `vendor/esvg-rs`).

### Key layers

- **`ui/*.slint`** — Slint UI definitions. `build.rs` compiles these into Rust via `slint_build::compile("ui/main-window.slint")`. All components are imported transitively from `main-window.slint`.
- **`ui/app-state.slint`** — The single global state singleton (`AppState`) shared across all UI components and the Rust backend. All plugin toggles, export options, image buffers, and loading state live here.
- **`src/main.rs`** — Entire Rust backend. Sets up the Slint window, wires callbacks (`on_open_file`, `on_request_optimize`, `on_export_file`), and drives background threads for SVG loading, optimization, and rendering.
- **`vendor/esvg-rs`** — The SVG optimization library (git submodule). Exposes `optimize()`, `PluginConfig`, `apply_svg_export_options()`, and `SvgExportOptions`.

### Data flow

1. User opens a file → `on_open_file` callback → `load_svg()` runs on a background thread
2. `load_svg` renders original SVG at 1× and 6× (via `resvg`/`tiny_skia`), optimizes with default `PluginConfig`, renders optimized versions, pushes results to UI via `slint::invoke_from_event_loop`
3. When plugin toggles change in the sidebar → `AppState.request_optimize()` callback → `render_optimized()` re-runs optimization+rendering with updated `PluginConfig` built from current `AppState`
4. Export → `AppState.export_file(format)` → saves optimized SVG or renders to PNG at user-specified dimensions

### Rendering pipeline

SVGs are rendered with `resvg` (backed by `tiny_skia`/Skia). Two resolutions are always produced: 1× (shown immediately) and 6× (high quality, replaces 1× once ready). The `zoom_enabled` flag gates zoom interaction until the 6× render completes.

### Cancellation

`render_optimized` uses an `Arc<AtomicU64>` generation counter to cancel stale in-flight renders when the user changes plugin settings rapidly.

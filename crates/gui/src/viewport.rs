//! Renders only the visible part of the SVG, at device resolution, on a background thread.
//!
//! The viewer asks for a frame whenever the view settles (or a pan uncovers an edge). The
//! worker always renders the latest request, dropping intermediate ones, and renders the
//! variant on screen first so toggling original/optimized is instant afterwards.

use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use resvg::{tiny_skia, usvg};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

use crate::{AppState, AppWindow, ViewFrame};

/// Extra area rendered around the viewport so short pans don't uncover blank space,
/// as a fraction of the viewport size, capped in logical pixels.
const OVERSCAN_FRACTION: f32 = 0.25;
const OVERSCAN_MAX: f32 = 384.0;

/// Longest side of the whole-document overview shown while a frame is pending.
const OVERVIEW_MAX_SIZE: f32 = 2048.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Original = 0,
    Optimized = 1,
}

/// The view, in logical pixels, as the Slint viewer sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewRequest {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub scale_factor: f32,
    pub show_optimized: bool,
}

/// A rendered frame. `x`/`y`/`width`/`height` are in SVG user units.
pub struct FrameData {
    pub pixels: SharedPixelBuffer<Rgba8Pixel>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Integer physical-pixel rectangle, in viewport coordinates, that a frame covers.
#[derive(Debug, PartialEq)]
pub struct PixelRegion {
    pub left: f32,
    pub top: f32,
    pub width: u32,
    pub height: u32,
}

/// The visible part of the SVG plus overscan, snapped to whole device pixels so the frame
/// is shown 1:1 without resampling. `None` when the SVG is entirely off screen.
pub fn frame_region(svg_width: f32, svg_height: f32, view: &ViewRequest) -> Option<PixelRegion> {
    let sf = view.scale_factor;
    let scale = view.zoom * sf;
    if !(scale.is_finite() && scale > 0.0 && svg_width > 0.0 && svg_height > 0.0) {
        return None;
    }
    let overscan_x = (view.viewport_width * OVERSCAN_FRACTION).min(OVERSCAN_MAX) * sf;
    let overscan_y = (view.viewport_height * OVERSCAN_FRACTION).min(OVERSCAN_MAX) * sf;

    let svg_left = view.pan_x * sf;
    let svg_top = view.pan_y * sf;
    let left = svg_left.max(-overscan_x).floor();
    let top = svg_top.max(-overscan_y).floor();
    let right = (svg_left + svg_width * scale)
        .min(view.viewport_width * sf + overscan_x)
        .ceil();
    let bottom = (svg_top + svg_height * scale)
        .min(view.viewport_height * sf + overscan_y)
        .ceil();
    if right <= left || bottom <= top {
        return None;
    }
    Some(PixelRegion {
        left,
        top,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    })
}

// tiny-skia writes premultiplied RGBA; pass buffers to Image::from_rgba8_premultiplied.
pub fn render_frame(tree: &usvg::Tree, view: &ViewRequest) -> Option<FrameData> {
    let size = tree.size();
    let region = frame_region(size.width(), size.height(), view)?;
    let sf = view.scale_factor;
    let scale = view.zoom * sf;

    let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(region.width, region.height);
    {
        let mut pixmap =
            tiny_skia::PixmapMut::from_bytes(pixels.make_mut_bytes(), region.width, region.height)?;
        resvg::render(
            tree,
            tiny_skia::Transform::from_row(
                scale,
                0.0,
                0.0,
                scale,
                view.pan_x * sf - region.left,
                view.pan_y * sf - region.top,
            ),
            &mut pixmap,
        );
    }

    Some(FrameData {
        pixels,
        x: (region.left / sf - view.pan_x) / view.zoom,
        y: (region.top / sf - view.pan_y) / view.zoom,
        width: region.width as f32 / scale,
        height: region.height as f32 / scale,
    })
}

/// The whole document at a bounded size, shown wherever no sharp frame is available yet.
pub fn render_overview(tree: &usvg::Tree) -> SharedPixelBuffer<Rgba8Pixel> {
    let size = tree.size();
    let scale = OVERVIEW_MAX_SIZE / size.width().max(size.height());
    let width = ((size.width() * scale).ceil() as u32).max(1);
    let height = ((size.height() * scale).ceil() as u32).max(1);

    let mut pixels = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    if let Some(mut pixmap) =
        tiny_skia::PixmapMut::from_bytes(pixels.make_mut_bytes(), width, height)
    {
        resvg::render(
            tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap,
        );
    }
    pixels
}

#[derive(Default)]
struct State {
    trees: [Option<Arc<usvg::Tree>>; 2],
    /// Bumped whenever a tree changes so frames of a replaced tree are dropped.
    generations: [u64; 2],
    last_request: Option<ViewRequest>,
    pending: bool,
}

pub struct ViewportRenderer {
    state: Mutex<State>,
    wake: Condvar,
}

impl ViewportRenderer {
    pub fn start(ui: slint::Weak<AppWindow>) -> Arc<Self> {
        let renderer = Arc::new(Self {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
        });
        let worker = Arc::clone(&renderer);
        thread::spawn(move || worker.run(ui));
        renderer
    }

    pub fn request(&self, view: ViewRequest) {
        let mut state = self.state.lock().unwrap();
        state.last_request = Some(view);
        state.pending = true;
        self.wake.notify_one();
    }

    /// Replaces one variant's tree. Call on the UI thread, after clearing that variant's frame,
    /// so a frame from the previous tree can never land after the clear.
    pub fn set_tree(&self, variant: Variant, tree: Option<Arc<usvg::Tree>>) {
        let mut state = self.state.lock().unwrap();
        let has_tree = tree.is_some();
        state.trees[variant as usize] = tree;
        state.generations[variant as usize] += 1;
        if has_tree && state.last_request.is_some() {
            state.pending = true;
            self.wake.notify_one();
        }
    }

    /// Forgets both trees and the last view, e.g. when a new file is opened.
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.trees = [None, None];
        state.generations[0] += 1;
        state.generations[1] += 1;
        state.last_request = None;
        state.pending = false;
    }

    fn run(self: Arc<Self>, ui: slint::Weak<AppWindow>) {
        loop {
            let (view, jobs) = {
                let mut state = self.state.lock().unwrap();
                while !state.pending {
                    state = self.wake.wait(state).unwrap();
                }
                state.pending = false;
                let Some(view) = state.last_request else {
                    continue;
                };
                let order = if view.show_optimized {
                    [Variant::Optimized, Variant::Original]
                } else {
                    [Variant::Original, Variant::Optimized]
                };
                let jobs: Vec<_> = order
                    .into_iter()
                    .filter_map(|variant| {
                        let index = variant as usize;
                        state.trees[index]
                            .clone()
                            .map(|tree| (variant, tree, state.generations[index]))
                    })
                    .collect();
                (view, jobs)
            };

            for (variant, tree, generation) in jobs {
                // A newer view supersedes the rest of this one.
                if self.state.lock().unwrap().pending {
                    break;
                }
                let Some(frame) = render_frame(&tree, &view) else {
                    continue;
                };
                let renderer = Arc::clone(&self);
                let ui = ui.clone();
                slint::invoke_from_event_loop(move || {
                    if renderer.state.lock().unwrap().generations[variant as usize] != generation {
                        return;
                    }
                    if let Some(ui) = ui.upgrade() {
                        set_frame(&ui.global::<AppState>(), variant, frame_to_slint(frame));
                    }
                })
                .ok();
            }
        }
    }
}

fn frame_to_slint(frame: FrameData) -> ViewFrame {
    ViewFrame {
        image: Image::from_rgba8_premultiplied(frame.pixels),
        x: frame.x,
        y: frame.y,
        width: frame.width,
        height: frame.height,
    }
}

pub fn set_frame(state: &AppState, variant: Variant, frame: ViewFrame) {
    match variant {
        Variant::Original => state.set_frame_original(frame),
        Variant::Optimized => state.set_frame_optimized(frame),
    }
}

/// Shows a new overview for `variant` and hands the tree to the renderer. The previous frame
/// stays up until the new tree's frame replaces it, which avoids a blurry flash when plugin
/// toggles re-optimize. Must run on the UI thread.
pub fn show_document(
    ui: &AppWindow,
    renderer: &ViewportRenderer,
    variant: Variant,
    tree: Arc<usvg::Tree>,
    overview: SharedPixelBuffer<Rgba8Pixel>,
) {
    let state = ui.global::<AppState>();
    let overview = Image::from_rgba8_premultiplied(overview);
    match variant {
        Variant::Original => state.set_svg_overview_original(overview),
        Variant::Optimized => state.set_svg_overview_optimized(overview),
    }
    renderer.set_tree(variant, Some(tree));
}

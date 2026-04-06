// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use resvg::{tiny_skia, usvg};
slint::include_modules!();
use esvg_rs::{apply_svg_export_options, optimize, PluginConfig, SvgExportOptions};
use rfd::AsyncFileDialog;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

fn make_checker_image() -> SharedPixelBuffer<Rgba8Pixel> {
    const SIZE: u32 = 256;
    const CELL: u32 = 8;
    const LIGHT: [u8; 4] = [0x4a, 0x4a, 0x4a, 0xff];
    const DARK: [u8; 4] = [0x33, 0x33, 0x33, 0xff];

    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(SIZE, SIZE);
    let pixels = buf.make_mut_bytes();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let color = if ((x / CELL) + (y / CELL)) % 2 == 0 { LIGHT } else { DARK };
            let i = ((y * SIZE + x) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&color);
        }
    }
    buf
}

fn render_svg(tree: &usvg::Tree, zoom: f32) -> SharedPixelBuffer<Rgba8Pixel> {
    let width = tree.size().width();
    let height = tree.size().height();

    let scaled_width = ((width * zoom).ceil() as u32).max(1);
    let scaled_height = ((height * zoom).ceil() as u32).max(1);

    let mut pixmap = tiny_skia::Pixmap::new(scaled_width, scaled_height).unwrap();
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(zoom, zoom),
        &mut pixmap.as_mut(),
    );

    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(scaled_width, scaled_height);
    buffer.make_mut_bytes().copy_from_slice(pixmap.data());

    buffer
}

fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn build_plugin_config(state: &AppState) -> PluginConfig {
    PluginConfig {
        apply_transforms: state.get_plugin_apply_transforms(),
        collapse_groups: state.get_plugin_collapse_groups(),
        combine_paths: state.get_plugin_combine_paths(),
        css_to_attributes: state.get_plugin_css_to_attributes(),
        mangle_ids: state.get_plugin_mangle_ids(),
        number_precision: state.get_plugin_number_precision(),
        optimize_colors: state.get_plugin_optimize_colors(),
        remove_empty_text: state.get_plugin_remove_empty_text(),
        remove_unnecessary_attrs: state.get_plugin_remove_unnecessary_attrs(),
        remove_unnecessary_clip_paths: state.get_plugin_remove_unnecessary_clip_paths(),
        shape_to_path: state.get_plugin_shape_to_path(),
        simplify_paths: state.get_plugin_simplify_paths(),
        sort_attrs: state.get_plugin_sort_attrs(),
    }
}

fn render_optimized(
    ui_handle: slint::Weak<AppWindow>,
    svg_string: String,
    config: PluginConfig,
    generation: Arc<AtomicU64>,
    optimized_store: Arc<Mutex<String>>,
) {
    let my_gen = generation.fetch_add(1, Ordering::SeqCst) + 1;
    let ui_handle2 = ui_handle.clone();

    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let state = ui.global::<AppState>();
            state.set_zoom_enabled(false);
            state.set_loading(true);
            state.set_loading_status("Optimizing...".into());
        }
    })
    .ok();

    thread::spawn(move || {
        macro_rules! cancelled {
            () => {
                generation.load(Ordering::SeqCst) != my_gen
            };
        }
        macro_rules! cancel_return {
            () => {
                if cancelled!() {
                    slint::invoke_from_event_loop({
                        let h = ui_handle2.clone();
                        move || {
                            if let Some(ui) = h.upgrade() {
                                ui.global::<AppState>().set_loading(false);
                            }
                        }
                    })
                    .ok();
                    return;
                }
            };
        }

        let optimized_str = optimize(&svg_string, &config).unwrap_or_else(|_| svg_string.clone());
        *optimized_store.lock().unwrap() = optimized_str.clone();
        cancel_return!();

        let optimized_size = optimized_str.len();
        let optimized_tree = Arc::new(
            usvg::Tree::from_data(optimized_str.as_bytes(), &usvg::Options::default()).unwrap(),
        );

        let h = ui_handle2.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(ui) = h.upgrade() {
                let state = ui.global::<AppState>();
                state.set_optimized_size(format_size(optimized_size).into());
                state.set_loading_status("Rendering...".into());
            }
        })
        .ok();

        let opt_1x = render_svg(&optimized_tree, 1.0);
        cancel_return!();
        let h = ui_handle2.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(ui) = h.upgrade() {
                ui.global::<AppState>()
                    .set_svg_image_optimized(Image::from_rgba8(opt_1x));
            }
        })
        .ok();

        let opt_6x = render_svg(&optimized_tree, 6.0);
        cancel_return!();
        slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_handle2.upgrade() {
                let state = ui.global::<AppState>();
                state.set_svg_image_optimized(Image::from_rgba8(opt_6x));
                state.set_zoom_enabled(true);
                state.set_loading(false);
            }
        })
        .ok();
    });
}

fn load_svg(
    ui_handle: slint::Weak<AppWindow>,
    svg_store: Arc<Mutex<String>>,
    optimized_store: Arc<Mutex<String>>,
    path: std::path::PathBuf,
) {
    thread::spawn(move || {
        // 1. Read + parse original
        let svg_bytes = std::fs::read(&path).unwrap();
        let svg_string = String::from_utf8_lossy(&svg_bytes).into_owned();
        let original_size = svg_bytes.len();
        let original_tree =
            Arc::new(usvg::Tree::from_data(&svg_bytes, &usvg::Options::default()).unwrap());
        let width = original_tree.size().width();
        let height = original_tree.size().height();

        *svg_store.lock().unwrap() = svg_string.clone();

        // Build SharedString on the background thread — only a refcount bump crosses to main.
        let h = ui_handle.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(ui) = h.upgrade() {
                let state = ui.global::<AppState>();
                state.set_svg_width(width);
                state.set_svg_height(height);
                state.set_export_svg_natural_width(width);
                state.set_export_svg_natural_height(height);
                state.set_export_svg_width(width);
                state.set_export_svg_height(height);
                state.set_export_png_width(width);
                state.set_export_png_height(height);
                state.set_original_size(format_size(original_size).into());
                state.set_loading_status("Rendering original...".into());
            }
        })
        .ok();

        // 2. Original @ 1x
        let orig_1x = render_svg(&original_tree, 1.0);
        let h = ui_handle.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(ui) = h.upgrade() {
                let state = ui.global::<AppState>();
                state.set_svg_image_original(Image::from_rgba8(orig_1x));
                state.set_loading_status("Optimizing...".into());
            }
        })
        .ok();

        // 3. Optimize
        let config = PluginConfig::default();
        let optimized_str = optimize(&svg_string, &config).unwrap_or_else(|_| svg_string.clone());
        *optimized_store.lock().unwrap() = optimized_str.clone();
        let optimized_size = optimized_str.len();
        let optimized_tree = Arc::new(
            usvg::Tree::from_data(optimized_str.as_bytes(), &usvg::Options::default()).unwrap(),
        );

        let h = ui_handle.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(ui) = h.upgrade() {
                let state = ui.global::<AppState>();
                state.set_optimized_size(format_size(optimized_size).into());
                state.set_loading_status("Rendering optimized...".into());
            }
        })
        .ok();

        // 4. Optimized @ 1x → close dialog, mark loaded, switch to status bar
        let opt_1x = render_svg(&optimized_tree, 1.0);
        let h = ui_handle.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(ui) = h.upgrade() {
                let state = ui.global::<AppState>();
                state.set_svg_image_optimized(Image::from_rgba8(opt_1x));
                state.set_svg_loaded(true);
                state.set_loading_dialog(false);
                state.set_loading_status("Rendering high quality...".into());
            }
        })
        .ok();

        // 5. Original @ 6x
        let orig_6x = render_svg(&original_tree, 6.0);
        let h = ui_handle.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(ui) = h.upgrade() {
                ui.global::<AppState>()
                    .set_svg_image_original(Image::from_rgba8(orig_6x));
            }
        })
        .ok();

        // 6. Optimized @ 6x → enable zoom, clear status bar
        let opt_6x = render_svg(&optimized_tree, 6.0);
        slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let state = ui.global::<AppState>();
                state.set_svg_image_optimized(Image::from_rgba8(opt_6x));
                state.set_zoom_enabled(true);
                state.set_loading(false);
            }
        })
        .ok();
    });
}

fn main() -> Result<(), Box<dyn Error>> {
    let ui = AppWindow::new()?;

    ui.global::<AppState>()
        .set_checker_image(Image::from_rgba8(make_checker_image()));

    let svg_store: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let optimized_store: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let optimize_generation: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

    // Open file callback
    let ui_handle = ui.as_weak();

    let svg_store_for_open = Arc::clone(&svg_store);
    let optimized_store_for_open = Arc::clone(&optimized_store);
    let optimize_gen_for_open = Arc::clone(&optimize_generation);

    ui.on_open_file(move || {
        let ui_handle2 = ui_handle.clone();
        let svg_store = Arc::clone(&svg_store_for_open);
        let optimized_store = Arc::clone(&optimized_store_for_open);
        let optimize_gen = Arc::clone(&optimize_gen_for_open);
        slint::spawn_local(async move {
            let Some(file) = AsyncFileDialog::new()
                .add_filter("SVG", &["svg"])
                .set_directory("/")
                .pick_file()
                .await
            else {
                return;
            };
            let path = file.path().to_path_buf();

            // Invalidate any in-flight re-optimize from the previous file
            optimize_gen.fetch_add(1, Ordering::SeqCst);

            // Show loading dialog immediately before spawning the background thread
            if let Some(ui) = ui_handle2.upgrade() {
                let state = ui.global::<AppState>();
                state.set_loading_dialog(true);
                state.set_loading(true);
                state.set_loading_status("Reading file...".into());
                state.set_zoom_enabled(false);
                state.set_svg_loaded(false);
                state.set_svg_image_original(Default::default());
                state.set_svg_image_optimized(Default::default());

                // Reset all plugin toggles to enabled for the new file
                state.set_plugin_apply_transforms(true);
                state.set_plugin_collapse_groups(true);
                state.set_plugin_combine_paths(true);
                state.set_plugin_css_to_attributes(true);
                state.set_plugin_mangle_ids(true);
                state.set_plugin_number_precision(true);
                state.set_plugin_optimize_colors(true);
                state.set_plugin_remove_empty_text(true);
                state.set_plugin_remove_unnecessary_attrs(true);
                state.set_plugin_remove_unnecessary_clip_paths(true);
                state.set_plugin_shape_to_path(true);
                state.set_plugin_simplify_paths(true);
                state.set_plugin_sort_attrs(true);
            }

            load_svg(ui_handle2.clone(), Arc::clone(&svg_store), Arc::clone(&optimized_store), path);
        }).ok();
    });

    // Re-optimize callback (triggered when plugin switches are toggled)
    let ui_handle = ui.as_weak();
    let svg_store_for_optimize = Arc::clone(&svg_store);
    let optimized_store_for_optimize = Arc::clone(&optimized_store);
    let optimize_gen_for_callback = Arc::clone(&optimize_generation);
    ui.global::<AppState>().on_request_optimize(move || {
        let svg_str = svg_store_for_optimize.lock().unwrap().clone();
        if svg_str.is_empty() {
            return;
        }
        let config = if let Some(ui) = ui_handle.upgrade() {
            build_plugin_config(&ui.global::<AppState>())
        } else {
            return;
        };
        render_optimized(
            ui_handle.clone(),
            svg_str,
            config,
            Arc::clone(&optimize_gen_for_callback),
            Arc::clone(&optimized_store_for_optimize),
        );
    });

    // Export callback
    let optimized_store_for_export = Arc::clone(&optimized_store);
    let ui_handle = ui.as_weak();
    ui.global::<AppState>().on_export_file(move |format| {
        let optimized_store = Arc::clone(&optimized_store_for_export);
        let format = format.to_string();

        // Snapshot all export options from AppState on the main thread before entering async block
        let (svg_include_viewbox, svg_include_dimensions, svg_export_width, svg_export_height,
             png_export_width, png_export_height) = if let Some(ui) = ui_handle.upgrade() {
            let state = ui.global::<AppState>();
            (
                state.get_export_svg_include_viewbox(),
                state.get_export_svg_include_dimensions(),
                state.get_export_svg_width(),
                state.get_export_svg_height(),
                state.get_export_png_width(),
                state.get_export_png_height(),
            )
        } else {
            return;
        };

        let ui_handle_for_export = ui_handle.clone();
        slint::spawn_local(async move {
            let file = match format.as_str() {
                "PNG" => AsyncFileDialog::new()
                    .add_filter("PNG Image", &["png"])
                    .save_file()
                    .await,
                _ => AsyncFileDialog::new()
                    .add_filter("SVG Image", &["svg"])
                    .save_file()
                    .await,
            };
            let Some(file) = file else { return };
            let path = file.path().to_path_buf();

            let optimized_svg = optimized_store.lock().unwrap().clone();
            if optimized_svg.is_empty() {
                return;
            }

            if let Some(ui) = ui_handle_for_export.upgrade() {
                let state = ui.global::<AppState>();
                state.set_loading(true);
                state.set_loading_status("Exporting...".into());
            }

            match format.as_str() {
                "PNG" => {
                    let h = ui_handle_for_export.clone();
                    thread::spawn(move || {
                        let tree = match usvg::Tree::from_data(
                            optimized_svg.as_bytes(),
                            &usvg::Options::default(),
                        ) {
                            Ok(t) => t,
                            Err(_) => return,
                        };
                        let natural_w = tree.size().width();
                        let natural_h = tree.size().height();

                        // Use user-specified dimensions, falling back to natural size
                        let target_w = if png_export_width > 0.0 { png_export_width } else { natural_w };
                        let target_h = if png_export_height > 0.0 { png_export_height } else { natural_h };

                        let out_w = (target_w.ceil() as u32).max(1);
                        let out_h = (target_h.ceil() as u32).max(1);

                        let scale_x = target_w / natural_w;
                        let scale_y = target_h / natural_h;

                        let mut pixmap = tiny_skia::Pixmap::new(out_w, out_h).unwrap();
                        resvg::render(
                            &tree,
                            tiny_skia::Transform::from_scale(scale_x, scale_y),
                            &mut pixmap.as_mut(),
                        );
                        if let Ok(png_data) = pixmap.encode_png() {
                            std::fs::write(&path, png_data).ok();
                        }
                        slint::invoke_from_event_loop(move || {
                            if let Some(ui) = h.upgrade() {
                                ui.global::<AppState>().set_loading(false);
                            }
                        }).ok();
                    });
                }
                _ => {
                    let h = ui_handle_for_export.clone();
                    thread::spawn(move || {
                        let opts = SvgExportOptions {
                            include_viewbox: svg_include_viewbox,
                            include_dimensions: svg_include_dimensions,
                            width: svg_export_width,
                            height: svg_export_height,
                        };
                        let output = apply_svg_export_options(&optimized_svg, &opts)
                            .unwrap_or(optimized_svg);
                        std::fs::write(&path, output.as_bytes()).ok();
                        slint::invoke_from_event_loop(move || {
                            if let Some(ui) = h.upgrade() {
                                ui.global::<AppState>().set_loading(false);
                            }
                        }).ok();
                    });
                }
            }
        }).ok();
    });

    ui.run()?;

    Ok(())
}

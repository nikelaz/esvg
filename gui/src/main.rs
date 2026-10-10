// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use resvg::{tiny_skia, usvg};
slint::include_modules!();
use esvg_rs::{apply_svg_export_options, optimize, PluginConfig, SvgExportOptions};
use rfd::AsyncFileDialog;
use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

fn make_checker_image() -> SharedPixelBuffer<Rgba8Pixel> {
    const SIZE: u32 = 256;
    const CELL: u32 = 8;
    const LIGHT: [u8; 4] = [0x4a, 0x4a, 0x4a, 0xff];
    const DARK: [u8; 4] = [0x33, 0x33, 0x33, 0xff];

    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(SIZE, SIZE);
    let pixels = buf.make_mut_bytes();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let color = if ((x / CELL) + (y / CELL)) % 2 == 0 {
                LIGHT
            } else {
                DARK
            };
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

const BULK_PREVIEW_MAX_EDGE: f32 = 1024.0;
const BULK_PREVIEW_MAX_SCALE: f32 = 2.0;

struct BulkFileData {
    path: PathBuf,
    source_svg: Option<String>,
    optimized_svg: Option<String>,
    generation: Arc<AtomicU64>,
}

fn new_bulk_svg(name: String) -> BulkSvg {
    BulkSvg {
        name: name.into(),
        status: "Reading…".into(),
        original_size: "".into(),
        optimized_size: "".into(),
        original_image: Image::default(),
        optimized_image: Image::default(),
        width: 0.0,
        height: 0.0,
        expanded: false,
        plugin_apply_transforms: true,
        plugin_collapse_groups: true,
        plugin_combine_paths: true,
        plugin_css_to_attributes: true,
        plugin_mangle_ids: true,
        plugin_number_precision: true,
        plugin_optimize_colors: true,
        plugin_remove_empty_text: true,
        plugin_remove_unnecessary_attrs: true,
        plugin_remove_unnecessary_clip_paths: true,
        plugin_shape_to_path: true,
        plugin_simplify_paths: true,
        plugin_sort_attrs: true,
    }
}

fn build_bulk_plugin_config(file: &BulkSvg) -> PluginConfig {
    PluginConfig {
        apply_transforms: file.plugin_apply_transforms,
        collapse_groups: file.plugin_collapse_groups,
        combine_paths: file.plugin_combine_paths,
        css_to_attributes: file.plugin_css_to_attributes,
        mangle_ids: file.plugin_mangle_ids,
        number_precision: file.plugin_number_precision,
        optimize_colors: file.plugin_optimize_colors,
        remove_empty_text: file.plugin_remove_empty_text,
        remove_unnecessary_attrs: file.plugin_remove_unnecessary_attrs,
        remove_unnecessary_clip_paths: file.plugin_remove_unnecessary_clip_paths,
        shape_to_path: file.plugin_shape_to_path,
        simplify_paths: file.plugin_simplify_paths,
        sort_attrs: file.plugin_sort_attrs,
    }
}

fn copy_bulk_plugins_to_file(file: &mut BulkSvg, ui: &BulkWindow) {
    file.plugin_apply_transforms = ui.get_plugin_apply_transforms();
    file.plugin_collapse_groups = ui.get_plugin_collapse_groups();
    file.plugin_combine_paths = ui.get_plugin_combine_paths();
    file.plugin_css_to_attributes = ui.get_plugin_css_to_attributes();
    file.plugin_mangle_ids = ui.get_plugin_mangle_ids();
    file.plugin_number_precision = ui.get_plugin_number_precision();
    file.plugin_optimize_colors = ui.get_plugin_optimize_colors();
    file.plugin_remove_empty_text = ui.get_plugin_remove_empty_text();
    file.plugin_remove_unnecessary_attrs = ui.get_plugin_remove_unnecessary_attrs();
    file.plugin_remove_unnecessary_clip_paths = ui.get_plugin_remove_unnecessary_clip_paths();
    file.plugin_shape_to_path = ui.get_plugin_shape_to_path();
    file.plugin_simplify_paths = ui.get_plugin_simplify_paths();
    file.plugin_sort_attrs = ui.get_plugin_sort_attrs();
}

fn copy_bulk_plugins_to_ui(ui: &BulkWindow, file: &BulkSvg) {
    ui.set_plugin_apply_transforms(file.plugin_apply_transforms);
    ui.set_plugin_collapse_groups(file.plugin_collapse_groups);
    ui.set_plugin_combine_paths(file.plugin_combine_paths);
    ui.set_plugin_css_to_attributes(file.plugin_css_to_attributes);
    ui.set_plugin_mangle_ids(file.plugin_mangle_ids);
    ui.set_plugin_number_precision(file.plugin_number_precision);
    ui.set_plugin_optimize_colors(file.plugin_optimize_colors);
    ui.set_plugin_remove_empty_text(file.plugin_remove_empty_text);
    ui.set_plugin_remove_unnecessary_attrs(file.plugin_remove_unnecessary_attrs);
    ui.set_plugin_remove_unnecessary_clip_paths(file.plugin_remove_unnecessary_clip_paths);
    ui.set_plugin_shape_to_path(file.plugin_shape_to_path);
    ui.set_plugin_simplify_paths(file.plugin_simplify_paths);
    ui.set_plugin_sort_attrs(file.plugin_sort_attrs);
}

fn render_bulk_preview(tree: &usvg::Tree) -> SharedPixelBuffer<Rgba8Pixel> {
    let width = tree.size().width();
    let height = tree.size().height();
    let scale = (BULK_PREVIEW_MAX_EDGE / width.max(height)).min(BULK_PREVIEW_MAX_SCALE);
    render_svg(tree, scale)
}

fn set_bulk_status(ui_handle: slint::Weak<BulkWindow>, index: usize, status: String) {
    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let files = ui.get_files();
            if let Some(mut file) = files.row_data(index) {
                file.status = status.into();
                files.set_row_data(index, file);
            }
        }
    })
    .ok();
}

fn load_bulk_file(
    ui_handle: slint::Weak<BulkWindow>,
    entries: Arc<Mutex<Vec<BulkFileData>>>,
    index: usize,
    path: PathBuf,
) {
    thread::spawn(move || {
        let svg_bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                set_bulk_status(ui_handle, index, "Could not read file".into());
                return;
            }
        };
        let original_tree = match usvg::Tree::from_data(&svg_bytes, &usvg::Options::default()) {
            Ok(tree) => tree,
            Err(_) => {
                set_bulk_status(ui_handle, index, "Invalid SVG".into());
                return;
            }
        };

        let source_svg = String::from_utf8_lossy(&svg_bytes).into_owned();
        let optimized_svg =
            optimize(&source_svg, &PluginConfig::default()).unwrap_or_else(|_| source_svg.clone());
        let optimized_tree =
            usvg::Tree::from_data(optimized_svg.as_bytes(), &usvg::Options::default())
                .unwrap_or_else(|_| original_tree.clone());
        let width = original_tree.size().width();
        let height = original_tree.size().height();
        let original_image = render_bulk_preview(&original_tree);
        let optimized_image = render_bulk_preview(&optimized_tree);
        let optimized_size = optimized_svg.len();

        if let Some(entry) = entries.lock().unwrap().get_mut(index) {
            entry.source_svg = Some(source_svg);
            entry.optimized_svg = Some(optimized_svg);
        }

        slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let files = ui.get_files();
                if let Some(mut file) = files.row_data(index) {
                    file.status = "Ready".into();
                    file.original_size = format_size(svg_bytes.len()).into();
                    file.optimized_size = format_size(optimized_size).into();
                    file.original_image = Image::from_rgba8(original_image);
                    file.optimized_image = Image::from_rgba8(optimized_image);
                    file.width = width;
                    file.height = height;
                    files.set_row_data(index, file);
                }
            }
        })
        .ok();
    });
}

fn add_bulk_files(
    ui_handle: slint::Weak<BulkWindow>,
    files: Rc<VecModel<BulkSvg>>,
    entries: Arc<Mutex<Vec<BulkFileData>>>,
    paths: Vec<PathBuf>,
) {
    for path in paths {
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        {
            continue;
        }

        let index = {
            let mut stored_entries = entries.lock().unwrap();
            if stored_entries.iter().any(|entry| entry.path == path) {
                continue;
            }
            let index = stored_entries.len();
            stored_entries.push(BulkFileData {
                path: path.clone(),
                source_svg: None,
                optimized_svg: None,
                generation: Arc::new(AtomicU64::new(0)),
            });
            index
        };

        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        files.push(new_bulk_svg(name));
        load_bulk_file(ui_handle.clone(), Arc::clone(&entries), index, path);
    }
}

fn decode_file_uri(uri: &str) -> Option<PathBuf> {
    let uri = uri.trim();
    if uri.is_empty() || uri.starts_with('#') {
        return None;
    }
    let mut value = uri.strip_prefix("file://")?;
    if value.starts_with("localhost/") {
        value = &value["localhost".len()..];
    } else if !value.starts_with('/') {
        if let Some(slash) = value.find('/') {
            value = &value[slash..];
        } else {
            return None;
        }
    }

    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16)? as u8;
            let low = (bytes[index + 2] as char).to_digit(16)? as u8;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    let decoded = String::from_utf8(decoded).ok()?;
    #[cfg(windows)]
    let decoded = decoded.strip_prefix('/').unwrap_or(&decoded).to_owned();
    Some(PathBuf::from(decoded))
}

fn bulk_drop_paths(data: &str) -> Vec<PathBuf> {
    data.lines().filter_map(decode_file_uri).collect()
}

fn output_svg_name(input_path: &Path, requested_suffix: &str) -> String {
    let stem = input_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "optimized".into());
    let mut suffix: String = requested_suffix
        .chars()
        .map(|character| {
            if character == '/' || character == '\\' {
                '_'
            } else {
                character
            }
        })
        .collect();
    if !suffix.to_ascii_lowercase().ends_with(".svg") {
        suffix.push_str(".svg");
    }
    format!("{stem}{suffix}")
}

struct BulkOptimizationJob {
    index: usize,
    source_svg: String,
    config: PluginConfig,
    generation: Arc<AtomicU64>,
    generation_id: u64,
}

fn run_bulk_optimization(
    ui_handle: slint::Weak<BulkWindow>,
    entries: Arc<Mutex<Vec<BulkFileData>>>,
    job: BulkOptimizationJob,
) {
    let optimized_svg =
        optimize(&job.source_svg, &job.config).unwrap_or_else(|_| job.source_svg.clone());
    let original_tree =
        match usvg::Tree::from_data(job.source_svg.as_bytes(), &usvg::Options::default()) {
            Ok(tree) => tree,
            Err(_) => return,
        };
    let optimized_tree = usvg::Tree::from_data(optimized_svg.as_bytes(), &usvg::Options::default())
        .unwrap_or_else(|_| original_tree.clone());
    let optimized_image = render_bulk_preview(&optimized_tree);
    let optimized_size = optimized_svg.len();

    if job.generation.load(Ordering::SeqCst) != job.generation_id {
        return;
    }
    if let Some(entry) = entries.lock().unwrap().get_mut(job.index) {
        entry.optimized_svg = Some(optimized_svg);
    }

    slint::invoke_from_event_loop(move || {
        if job.generation.load(Ordering::SeqCst) == job.generation_id {
            if let Some(ui) = ui_handle.upgrade() {
                let files = ui.get_files();
                if let Some(mut file) = files.row_data(job.index) {
                    file.status = "Ready".into();
                    file.optimized_size = format_size(optimized_size).into();
                    file.optimized_image = Image::from_rgba8(optimized_image);
                    files.set_row_data(job.index, file);
                }
            }
        }
    })
    .ok();
}

fn make_bulk_optimization_job(
    index: usize,
    file: &BulkSvg,
    entries: &Arc<Mutex<Vec<BulkFileData>>>,
) -> Option<BulkOptimizationJob> {
    let mut stored_entries = entries.lock().unwrap();
    let entry = stored_entries.get_mut(index)?;
    let source_svg = entry.source_svg.clone()?;
    let generation_id = entry.generation.fetch_add(1, Ordering::SeqCst) + 1;
    Some(BulkOptimizationJob {
        index,
        source_svg,
        config: build_bulk_plugin_config(file),
        generation: Arc::clone(&entry.generation),
        generation_id,
    })
}

#[derive(Clone)]
struct BulkExportOptions {
    format: String,
    include_viewbox: bool,
    include_dimensions: bool,
    width: f32,
    height: f32,
    png_width: f32,
    png_height: f32,
}

fn save_bulk_files(
    ui_handle: slint::Weak<BulkWindow>,
    files: Rc<VecModel<BulkSvg>>,
    entries: Arc<Mutex<Vec<BulkFileData>>>,
    suffix: String,
    export_options: BulkExportOptions,
) {
    slint::spawn_local(async move {
        let Some(folder) = AsyncFileDialog::new()
            .set_title("Choose destination for optimized SVGs")
            .pick_folder()
            .await
        else {
            return;
        };
        let folder = folder.path().to_path_buf();
        let has_active_work = (0..files.row_count()).any(|index| {
            files.row_data(index).is_some_and(|file| {
                matches!(file.status.to_string().as_str(), "Reading…" | "Optimizing…")
            })
        });
        if has_active_work {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_action_message(
                    "Wait for loading and optimization to finish before saving.".into(),
                );
            }
            return;
        }
        let mut outputs = Vec::new();
        for index in 0..files.row_count() {
            let Some(mut file) = files.row_data(index) else {
                continue;
            };
            if !matches!(
                file.status.to_string().as_str(),
                "Ready" | "Saved" | "Save failed"
            ) {
                continue;
            }
            let Some(job) = make_bulk_optimization_job(index, &file, &entries) else {
                continue;
            };
            let mut output_name = output_svg_name(&entries.lock().unwrap()[index].path, &suffix);
            if export_options.format == "PNG" {
                output_name = Path::new(&output_name)
                    .with_extension("png")
                    .to_string_lossy()
                    .into_owned();
            }
            let path = folder.join(output_name);
            file.status = "Optimizing…".into();
            files.set_row_data(index, file);
            outputs.push((job, path));
        }
        let ui_handle_for_saving = ui_handle.clone();
        thread::spawn(move || {
            for (job, path) in outputs {
                let index = job.index;
                run_bulk_optimization(ui_handle_for_saving.clone(), Arc::clone(&entries), job);
                let Some(svg) = entries.lock().unwrap()[index].optimized_svg.clone() else {
                    continue;
                };
                let output = if export_options.format == "PNG" {
                    usvg::Tree::from_data(svg.as_bytes(), &usvg::Options::default())
                        .ok()
                        .and_then(|tree| {
                            let natural_w = tree.size().width();
                            let natural_h = tree.size().height();
                            let target_w = if export_options.png_width > 0.0 {
                                export_options.png_width
                            } else {
                                natural_w
                            };
                            let target_h = if export_options.png_height > 0.0 {
                                export_options.png_height
                            } else {
                                natural_h
                            };
                            let mut pixmap = tiny_skia::Pixmap::new(
                                target_w.ceil().max(1.0) as u32,
                                target_h.ceil().max(1.0) as u32,
                            )?;
                            resvg::render(
                                &tree,
                                tiny_skia::Transform::from_scale(
                                    target_w / natural_w,
                                    target_h / natural_h,
                                ),
                                &mut pixmap.as_mut(),
                            );
                            pixmap.encode_png().ok()
                        })
                } else {
                    let tree =
                        usvg::Tree::from_data(svg.as_bytes(), &usvg::Options::default()).ok();
                    let (width, height) = tree
                        .as_ref()
                        .map(|tree| (tree.size().width(), tree.size().height()))
                        .unwrap_or((0.0, 0.0));
                    let opts = SvgExportOptions {
                        include_viewbox: export_options.include_viewbox,
                        include_dimensions: export_options.include_dimensions,
                        width: if export_options.width > 0.0 {
                            export_options.width
                        } else {
                            width
                        },
                        height: if export_options.height > 0.0 {
                            export_options.height
                        } else {
                            height
                        },
                    };
                    Some(
                        apply_svg_export_options(&svg, &opts)
                            .unwrap_or(svg)
                            .into_bytes(),
                    )
                };
                let status = if output.is_some_and(|bytes| std::fs::write(path, bytes).is_ok()) {
                    "Saved"
                } else {
                    "Save failed"
                };
                set_bulk_status(ui_handle_for_saving.clone(), index, status.into());
            }
        });
    })
    .ok();
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

    let checker_image = Image::from_rgba8(make_checker_image());
    ui.global::<AppState>()
        .set_checker_image(checker_image.clone());

    let svg_store: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let optimized_store: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let optimize_generation: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
    let bulk_window_slot: Rc<RefCell<Option<BulkWindow>>> = Rc::new(RefCell::new(None));

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

            load_svg(
                ui_handle2.clone(),
                Arc::clone(&svg_store),
                Arc::clone(&optimized_store),
                path,
            );
        })
        .ok();
    });

    let bulk_window_slot_for_open = Rc::clone(&bulk_window_slot);
    let checker_image_for_bulk = checker_image.clone();
    ui.on_open_bulk_window(move || {
        if let Some(bulk_window) = bulk_window_slot_for_open.borrow().as_ref() {
            let _ = bulk_window.show();
            return;
        }

        let bulk_window = match BulkWindow::new() {
            Ok(window) => window,
            Err(error) => {
                eprintln!("Could not open bulk optimizer: {error}");
                return;
            }
        };
        let files = Rc::new(VecModel::<BulkSvg>::default());
        let entries = Arc::new(Mutex::new(Vec::<BulkFileData>::new()));
        bulk_window.set_files(ModelRc::from(files.clone()));
        bulk_window.set_checker_image(checker_image_for_bulk.clone());

        let bulk_weak = bulk_window.as_weak();
        let files_for_open = Rc::clone(&files);
        let entries_for_open = Arc::clone(&entries);
        let weak_for_open = bulk_weak.clone();
        bulk_window.on_open_files(move || {
            let files = Rc::clone(&files_for_open);
            let entries = Arc::clone(&entries_for_open);
            let ui_handle = weak_for_open.clone();
            slint::spawn_local(async move {
                let Some(selected_files) = AsyncFileDialog::new()
                    .set_title("Add SVG files")
                    .add_filter("SVG", &["svg"])
                    .pick_files()
                    .await
                else {
                    return;
                };
                let paths = selected_files
                    .into_iter()
                    .map(|file| file.path().to_path_buf())
                    .collect();
                add_bulk_files(ui_handle, files, entries, paths);
            })
            .ok();
        });

        let files_for_drop = Rc::clone(&files);
        let entries_for_drop = Arc::clone(&entries);
        let weak_for_drop = bulk_weak.clone();
        bulk_window.on_dropped_files(move |data| {
            add_bulk_files(
                weak_for_drop.clone(),
                Rc::clone(&files_for_drop),
                Arc::clone(&entries_for_drop),
                bulk_drop_paths(&data.to_string()),
            );
        });

        let files_for_selection = Rc::clone(&files);
        let weak_for_selection = bulk_weak.clone();
        bulk_window.on_select_file(move |index| {
            if index < 0 {
                return;
            }
            let Some(mut file) = files_for_selection.row_data(index as usize) else {
                return;
            };
            file.expanded = !file.expanded;
            files_for_selection.set_row_data(index as usize, file.clone());
            if let Some(ui) = weak_for_selection.upgrade() {
                ui.set_selected_index(index);
                copy_bulk_plugins_to_ui(&ui, &file);
            }
        });

        let files_for_optimize = Rc::clone(&files);
        let entries_for_optimize = Arc::clone(&entries);
        let weak_for_optimize = bulk_weak.clone();
        bulk_window.on_request_optimize(move |index| {
            if index < 0 {
                return;
            }
            let Some(mut file) = files_for_optimize.row_data(index as usize) else {
                return;
            };
            if !matches!(
                file.status.to_string().as_str(),
                "Ready" | "Saved" | "Save failed"
            ) {
                return;
            }
            let Some(ui) = weak_for_optimize.upgrade() else {
                return;
            };
            copy_bulk_plugins_to_file(&mut file, &ui);
            let Some(job) =
                make_bulk_optimization_job(index as usize, &file, &entries_for_optimize)
            else {
                return;
            };
            file.status = "Optimizing…".into();
            files_for_optimize.set_row_data(index as usize, file);

            let ui_handle = weak_for_optimize.clone();
            let entries = Arc::clone(&entries_for_optimize);
            thread::spawn(move || run_bulk_optimization(ui_handle, entries, job));
        });

        let entries_for_save = Arc::clone(&entries);
        let weak_for_save = bulk_weak.clone();
        let files_for_save = Rc::clone(&files);
        bulk_window.on_optimize_and_save(move || {
            let Some(ui) = weak_for_save.upgrade() else {
                return;
            };
            let has_active_work = (0..files_for_save.row_count()).any(|index| {
                files_for_save.row_data(index).is_some_and(|file| {
                    matches!(file.status.to_string().as_str(), "Reading…" | "Optimizing…")
                })
            });
            if has_active_work {
                ui.set_action_message(
                    "Wait for loading and optimization to finish before saving.".into(),
                );
                return;
            }
            if !entries_for_save
                .lock()
                .unwrap()
                .iter()
                .any(|entry| entry.optimized_svg.is_some())
            {
                ui.set_action_message("There are no optimized SVGs ready to save.".into());
                return;
            }
            ui.set_action_message("".into());
            save_bulk_files(
                weak_for_save.clone(),
                Rc::clone(&files_for_save),
                Arc::clone(&entries_for_save),
                ui.get_output_suffix().to_string(),
                BulkExportOptions {
                    format: ui.get_output_format().to_string(),
                    include_viewbox: ui.get_export_include_viewbox(),
                    include_dimensions: ui.get_export_include_dimensions(),
                    width: ui.get_export_width(),
                    height: ui.get_export_height(),
                    png_width: ui.get_export_png_width(),
                    png_height: ui.get_export_png_height(),
                },
            );
        });

        let _ = bulk_window.show();
        *bulk_window_slot_for_open.borrow_mut() = Some(bulk_window);
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
        let (
            svg_include_viewbox,
            svg_include_dimensions,
            svg_export_width,
            svg_export_height,
            png_export_width,
            png_export_height,
        ) = if let Some(ui) = ui_handle.upgrade() {
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
                "PNG" => {
                    AsyncFileDialog::new()
                        .add_filter("PNG Image", &["png"])
                        .save_file()
                        .await
                }
                _ => {
                    AsyncFileDialog::new()
                        .add_filter("SVG Image", &["svg"])
                        .save_file()
                        .await
                }
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
                        let target_w = if png_export_width > 0.0 {
                            png_export_width
                        } else {
                            natural_w
                        };
                        let target_h = if png_export_height > 0.0 {
                            png_export_height
                        } else {
                            natural_h
                        };

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
                        })
                        .ok();
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
                        })
                        .ok();
                    });
                }
            }
        })
        .ok();
    });

    ui.run()?;

    Ok(())
}

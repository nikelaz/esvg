# Edge-case SVGs

Small hand-written SVG files, one per known way an optimizer can change how an
image looks. Each file targets a specific failure mode, for example:

| File | What it checks |
| --- | --- |
| `multi_segment_lines.svg`, `multi_curve_cubic.svg` | Commands that carry several segments (`L x y x y`, `c` with 12 values) |
| `collinear_overshoot.svg`, `simplify_smooth_zero.svg` | Curves that only look straight; zero-length `s`/`t` before a smooth command |
| `compact_arc_flags.svg`, `arc_mirror_flip.svg` | Arc flags written without separators; arcs under mirror and non-uniform scale |
| `transform_rotate_path.svg`, `rect_rx_scale.svg`, `circle_units_and_negative_scale.svg` | Transforms that must be left alone or baked correctly |
| `group_transform_clip.svg`, `element_transform_clip.svg`, `gradient_userspace_transform.svg`, `stroke_scale.svg` | Transforms that must not be baked because a clip, paint server or stroke depends on them |
| `group_visibility.svg`, `group_opacity_single_child.svg`, `group_collapse_attrs.svg` | Group collapsing that must keep inherited and non-inherited properties |
| `green_and_url_case.svg` | Colour rewrites that keep the exact colour and case-sensitive `url()` ids |
| `css_style_transform.svg`, `data_attr_css_selector.svg` | Inline styles and attribute selectors |
| `text_whitespace_tspan.svg` | Whitespace between `tspan` elements |
| `mangle_ids_*.svg` | Id and class renaming, including references that must not be renamed |
| `shape_units.svg`, `number_precision_small_viewbox.svg` | Unit values and very small viewBoxes |
| `clip_*.svg`, `combine_*.svg` | Clip-path removal and path combining at their boundaries |
| `combine_inherited_alpha.svg`, `transform_root_stroke.svg`, `shape_inherited_marker.svg` | Properties inherited from a group or the root `<svg>` |
| `group_transform_clip_child.svg`, `switch_group.svg` | A `<clipPath>` inside a transformed group; groups inside `<switch>` |
| `simplify_marker_vertices.svg` | Vertices that carry markers |
| `css_font_shorthand.svg` | A `font` shorthand next to a font longhand in an inline style |
| `clip_hidden_rect.svg`, `mangle_generated_collision.svg`, `precision_under_scale.svg` | A hidden clip shape; a generated class name that matches a stylesheet rule; rounding under `scale()` |

These files were written for this repository and are dedicated to the public
domain under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).

Run them with the renderer comparison in `tests/visual/run.py`.

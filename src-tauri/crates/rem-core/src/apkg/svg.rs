use crate::occlusion::{OcclusionShapeType, MAX_OCCLUSION_MASKS};

use super::html::{extract_html_attr, find_html_tag_end, style_hides_element};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SvgFill {
    None,
    Active,
    Inactive,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct RawSvgShape {
    pub(super) shape_type: OcclusionShapeType,
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) w: f64,
    pub(super) h: f64,
    pub(super) has_qshape_class: bool,
    pub(super) has_active_fill: bool,
}

#[derive(Debug, Clone, Copy)]
struct SvgGroupState {
    is_qshape: bool,
    fill: Option<SvgFill>,
    transform: [f64; 6],
    skip: bool,
}

const MAX_SVG_GROUP_DEPTH: usize = 64;
const IDENTITY_TRANSFORM: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn multiply_svg_transform(lhs: [f64; 6], rhs: [f64; 6]) -> [f64; 6] {
    [
        lhs[0] * rhs[0] + lhs[2] * rhs[1],
        lhs[1] * rhs[0] + lhs[3] * rhs[1],
        lhs[0] * rhs[2] + lhs[2] * rhs[3],
        lhs[1] * rhs[2] + lhs[3] * rhs[3],
        lhs[0] * rhs[4] + lhs[2] * rhs[5] + lhs[4],
        lhs[1] * rhs[4] + lhs[3] * rhs[5] + lhs[5],
    ]
}

fn apply_svg_transform(m: [f64; 6], x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn transform_bbox(m: [f64; 6], x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
    let corners = [
        apply_svg_transform(m, x, y),
        apply_svg_transform(m, x + w, y),
        apply_svg_transform(m, x, y + h),
        apply_svg_transform(m, x + w, y + h),
    ];
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (px, py) in corners {
        min_x = min_x.min(px);
        max_x = max_x.max(px);
        min_y = min_y.min(py);
        max_y = max_y.max(py);
    }
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

fn parse_svg_transform(raw: &str) -> [f64; 6] {
    let mut acc = IDENTITY_TRANSFORM;
    let mut rest = raw.trim();
    while let Some(open) = rest.find('(') {
        let name = rest[..open]
            .trim()
            .trim_start_matches(',')
            .trim()
            .to_ascii_lowercase();
        let after = &rest[open + 1..];
        let Some(close) = after.find(')') else {
            break;
        };
        let arg_bytes = &after.as_bytes()[..close];
        let mut arg_pos = 0usize;
        let mut args = Vec::new();
        while args.len() < 8 {
            let Some(val) = parse_svg_number_token(arg_bytes, &mut arg_pos) else {
                break;
            };
            args.push(val);
        }
        let next = match name.as_str() {
            "translate" if !args.is_empty() => {
                let tx = args[0];
                let ty = args.get(1).copied().unwrap_or(0.0);
                [1.0, 0.0, 0.0, 1.0, tx, ty]
            }
            "scale" if !args.is_empty() => {
                let sx = args[0];
                let sy = args.get(1).copied().unwrap_or(sx);
                [sx, 0.0, 0.0, sy, 0.0, 0.0]
            }
            "matrix" if args.len() >= 6 => [args[0], args[1], args[2], args[3], args[4], args[5]],
            "rotate" if !args.is_empty() => {
                let rad = args[0].to_radians();
                let (sin, cos) = rad.sin_cos();
                let rot = [cos, sin, -sin, cos, 0.0, 0.0];
                if args.len() >= 3 {
                    let cx = args[1];
                    let cy = args[2];
                    let t1 = [1.0, 0.0, 0.0, 1.0, cx, cy];
                    let t2 = [1.0, 0.0, 0.0, 1.0, -cx, -cy];
                    multiply_svg_transform(multiply_svg_transform(t1, rot), t2)
                } else {
                    rot
                }
            }
            _ => IDENTITY_TRANSFORM,
        };
        acc = multiply_svg_transform(acc, next);
        rest = &after[close + 1..];
    }
    acc
}

fn parse_svg_length(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    let numeric = trimmed
        .strip_suffix("px")
        .or_else(|| trimmed.strip_suffix("PX"))
        .or_else(|| trimmed.strip_suffix("pt"))
        .unwrap_or(trimmed)
        .trim();
    numeric.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn classify_svg_fill(raw: &str) -> Option<SvgFill> {
    let compact: String = raw
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    if compact.is_empty() {
        return None;
    }
    if compact == "none" || compact == "transparent" {
        Some(SvgFill::None)
    } else if matches!(
        compact.as_str(),
        "#ff7e7e" | "#ff7e7eff" | "rgb(255,126,126)"
    ) {
        Some(SvgFill::Active)
    } else {
        Some(SvgFill::Inactive)
    }
}

/// Extract effective SVG fill classification; per CSS precedence inline `style="fill:..."`
/// overrides presentation attribute `fill="..."`.
fn extract_svg_fill(tag_raw: &str) -> Option<SvgFill> {
    if let Some(style) = extract_html_attr(tag_raw, "style") {
        let mut style_fill = None;
        for decl in style.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                if k.trim().eq_ignore_ascii_case("fill") {
                    let val = v.trim();
                    let val = val.strip_suffix("!important").unwrap_or(val).trim();
                    if let Some(classified) = classify_svg_fill(val) {
                        style_fill = Some(classified);
                    }
                }
            }
        }
        if style_fill.is_some() {
            return style_fill;
        }
    }
    extract_html_attr(tag_raw, "fill").and_then(|fill| classify_svg_fill(&fill))
}

fn is_svg_element_hidden(tag_raw: &str) -> bool {
    if style_hides_element(tag_raw) {
        return true;
    }
    if extract_html_attr(tag_raw, "display").is_some_and(|v| v.trim().eq_ignore_ascii_case("none"))
    {
        return true;
    }
    if extract_html_attr(tag_raw, "visibility")
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("hidden"))
    {
        return true;
    }
    if extract_html_attr(tag_raw, "opacity")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .is_some_and(|o| o.is_finite() && o <= 0.0)
    {
        return true;
    }
    if let Some(style) = extract_html_attr(tag_raw, "style") {
        for decl in style.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                if k.trim().eq_ignore_ascii_case("opacity") {
                    let val = v.trim();
                    let val = val.strip_suffix("!important").unwrap_or(val).trim();
                    if val
                        .parse::<f64>()
                        .ok()
                        .is_some_and(|o| o.is_finite() && o <= 0.0)
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn is_svg_non_rendering_container(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "defs" | "clippath" | "mask" | "symbol" | "pattern" | "marker" | "style" | "metadata"
    )
}

fn has_qshape_class(tag_raw: &str) -> bool {
    extract_html_attr(tag_raw, "class").is_some_and(|cls| {
        cls.split_whitespace()
            .any(|tok| tok.eq_ignore_ascii_case("qshape"))
    })
}

fn parse_svg_canvas(tag_raw: &str) -> Option<(f64, f64, f64, f64)> {
    if let Some(vb) = extract_html_attr(tag_raw, "viewbox") {
        let nums: Vec<f64> = vb
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<f64>().ok())
            .filter(|n| n.is_finite())
            .collect();
        if nums.len() >= 4 && nums[2] > 0.0 && nums[3] > 0.0 {
            return Some((nums[0], nums[1], nums[2], nums[3]));
        }
    }
    let w = extract_html_attr(tag_raw, "width").and_then(|s| parse_svg_length(&s))?;
    let h = extract_html_attr(tag_raw, "height").and_then(|s| parse_svg_length(&s))?;
    if w > 0.0 && h > 0.0 {
        Some((0.0, 0.0, w, h))
    } else {
        None
    }
}

fn parse_svg_number_token(bytes: &[u8], pos: &mut usize) -> Option<f64> {
    let len = bytes.len();
    while *pos < len && (bytes[*pos].is_ascii_whitespace() || bytes[*pos] == b',') {
        *pos += 1;
    }
    if *pos >= len {
        return None;
    }
    let start = *pos;
    if bytes[*pos] == b'+' || bytes[*pos] == b'-' {
        *pos += 1;
    }
    let mut has_digits = false;
    while *pos < len && bytes[*pos].is_ascii_digit() {
        has_digits = true;
        *pos += 1;
    }
    if *pos < len && bytes[*pos] == b'.' {
        *pos += 1;
        while *pos < len && bytes[*pos].is_ascii_digit() {
            has_digits = true;
            *pos += 1;
        }
    }
    if !has_digits {
        *pos = start;
        return None;
    }
    if *pos < len && (bytes[*pos] == b'e' || bytes[*pos] == b'E') {
        let exp_start = *pos;
        *pos += 1;
        if *pos < len && (bytes[*pos] == b'+' || bytes[*pos] == b'-') {
            *pos += 1;
        }
        let mut exp_digits = false;
        while *pos < len && bytes[*pos].is_ascii_digit() {
            exp_digits = true;
            *pos += 1;
        }
        if !exp_digits {
            *pos = exp_start;
        }
    }
    std::str::from_utf8(&bytes[start..*pos])
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

fn parse_svg_flag_token(bytes: &[u8], pos: &mut usize) -> Option<bool> {
    let len = bytes.len();
    while *pos < len && (bytes[*pos].is_ascii_whitespace() || bytes[*pos] == b',') {
        *pos += 1;
    }
    if *pos >= len {
        return None;
    }
    match bytes[*pos] {
        b'0' => {
            *pos += 1;
            Some(false)
        }
        b'1' => {
            *pos += 1;
            Some(true)
        }
        _ => None,
    }
}

/// Sample points along an SVG elliptical arc (W3C SVG 1.1 Appendix F.6.5) so semicircular arcs
/// and rounded corners contribute their true bounding box extrema, not just endpoints.
#[allow(clippy::too_many_arguments)]
fn record_svg_arc_points<F: FnMut(f64, f64)>(
    x1: f64,
    y1: f64,
    rx_raw: f64,
    ry_raw: f64,
    x_rot_deg: f64,
    large_arc: bool,
    sweep: bool,
    x2: f64,
    y2: f64,
    record: &mut F,
) {
    record(x2, y2);
    let mut rx = rx_raw.abs();
    let mut ry = ry_raw.abs();
    if rx <= 1e-9 || ry <= 1e-9 || ((x1 - x2).abs() <= 1e-9 && (y1 - y2).abs() <= 1e-9) {
        return;
    }
    let phi = x_rot_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx2 = (x1 - x2) * 0.5;
    let dy2 = (y1 - y2) * 0.5;
    let x1p = cos_phi * dx2 + sin_phi * dy2;
    let y1p = -sin_phi * dx2 + cos_phi * dy2;

    let mut rx_sq = rx * rx;
    let mut ry_sq = ry * ry;
    let x1p_sq = x1p * x1p;
    let y1p_sq = y1p * y1p;

    let lambda = x1p_sq / rx_sq + y1p_sq / ry_sq;
    if lambda > 1.0 {
        let scale = lambda.sqrt();
        rx *= scale;
        ry *= scale;
        rx_sq = rx * rx;
        ry_sq = ry * ry;
    }

    let denom = rx_sq * y1p_sq + ry_sq * x1p_sq;
    if denom <= 1e-18 {
        return;
    }
    let num = (rx_sq * ry_sq - rx_sq * y1p_sq - ry_sq * x1p_sq).max(0.0);
    let coef = (num / denom).sqrt() * if large_arc == sweep { -1.0 } else { 1.0 };
    let cxp = coef * (rx * y1p / ry);
    let cyp = coef * (-ry * x1p / rx);

    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) * 0.5;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) * 0.5;

    let vec_angle = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let dot = ux * vx + uy * vy;
        let det = ux * vy - uy * vx;
        det.atan2(dot)
    };
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;

    let theta1 = vec_angle(1.0, 0.0, ux, uy);
    let mut dtheta = vec_angle(ux, uy, vx, vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= std::f64::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += std::f64::consts::TAU;
    }

    const SAMPLES: usize = 32;
    for step in 1..SAMPLES {
        let t = theta1 + dtheta * (step as f64 / SAMPLES as f64);
        let (sin_t, cos_t) = t.sin_cos();
        let px = cos_phi * rx * cos_t - sin_phi * ry * sin_t + cx;
        let py = sin_phi * rx * cos_t + cos_phi * ry * sin_t + cy;
        record(px, py);
    }
}

fn parse_svg_path_bbox(d: &str) -> Option<(f64, f64, f64, f64)> {
    let bytes = d.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;
    let mut cmd: Option<u8> = None;
    let mut cx = 0.0_f64;
    let mut cy = 0.0_f64;
    let mut sx = 0.0_f64;
    let mut sy = 0.0_f64;
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut steps = 0usize;

    let mut record = |x: f64, y: f64| {
        if x.is_finite() && y.is_finite() {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    };

    while pos < len && steps < 4096 {
        steps += 1;
        while pos < len && (bytes[pos].is_ascii_whitespace() || bytes[pos] == b',') {
            pos += 1;
        }
        if pos >= len {
            break;
        }
        if bytes[pos].is_ascii_alphabetic() {
            cmd = Some(bytes[pos]);
            pos += 1;
        }
        let Some(c) = cmd else {
            break;
        };
        match c {
            b'M' | b'm' => {
                let Some(x) = parse_svg_number_token(bytes, &mut pos) else {
                    break;
                };
                let Some(y) = parse_svg_number_token(bytes, &mut pos) else {
                    break;
                };
                if c == b'm' {
                    cx += x;
                    cy += y;
                } else {
                    cx = x;
                    cy = y;
                }
                sx = cx;
                sy = cy;
                record(cx, cy);
                // Subsequent coordinate pairs after M/m are implicit L/l.
                cmd = Some(if c == b'm' { b'l' } else { b'L' });
            }
            b'L' | b'l' | b'T' | b't' => {
                let Some(x) = parse_svg_number_token(bytes, &mut pos) else {
                    break;
                };
                let Some(y) = parse_svg_number_token(bytes, &mut pos) else {
                    break;
                };
                if c.is_ascii_lowercase() {
                    cx += x;
                    cy += y;
                } else {
                    cx = x;
                    cy = y;
                }
                record(cx, cy);
            }
            b'H' | b'h' => {
                let Some(x) = parse_svg_number_token(bytes, &mut pos) else {
                    break;
                };
                if c == b'h' {
                    cx += x;
                } else {
                    cx = x;
                }
                record(cx, cy);
            }
            b'V' | b'v' => {
                let Some(y) = parse_svg_number_token(bytes, &mut pos) else {
                    break;
                };
                if c == b'v' {
                    cy += y;
                } else {
                    cy = y;
                }
                record(cx, cy);
            }
            b'C' | b'c' => {
                let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                ) else {
                    break;
                };
                if c == b'c' {
                    record(cx + x1, cy + y1);
                    record(cx + x2, cy + y2);
                    cx += x;
                    cy += y;
                } else {
                    record(x1, y1);
                    record(x2, y2);
                    cx = x;
                    cy = y;
                }
                record(cx, cy);
            }
            b'S' | b's' | b'Q' | b'q' => {
                let (Some(x1), Some(y1), Some(x), Some(y)) = (
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                ) else {
                    break;
                };
                if c.is_ascii_lowercase() {
                    record(cx + x1, cy + y1);
                    cx += x;
                    cy += y;
                } else {
                    record(x1, y1);
                    cx = x;
                    cy = y;
                }
                record(cx, cy);
            }
            b'A' | b'a' => {
                let (Some(rx), Some(ry), Some(rot), Some(large), Some(sweep), Some(x), Some(y)) = (
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_flag_token(bytes, &mut pos),
                    parse_svg_flag_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                    parse_svg_number_token(bytes, &mut pos),
                ) else {
                    break;
                };
                let (ex, ey) = if c == b'a' { (cx + x, cy + y) } else { (x, y) };
                record_svg_arc_points(cx, cy, rx, ry, rot, large, sweep, ex, ey, &mut record);
                cx = ex;
                cy = ey;
            }
            b'Z' | b'z' => {
                cx = sx;
                cy = sy;
                cmd = None;
            }
            _ => break,
        }
    }

    if min_x.is_finite() && max_x > min_x && min_y.is_finite() && max_y > min_y {
        Some((min_x, min_y, max_x - min_x, max_y - min_y))
    } else {
        None
    }
}

fn parse_svg_polygon_bbox(points: &str) -> Option<(f64, f64, f64, f64)> {
    let bytes = points.as_bytes();
    let mut pos = 0;
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut count = 0usize;
    while let (Some(x), Some(y)) = (
        parse_svg_number_token(bytes, &mut pos),
        parse_svg_number_token(bytes, &mut pos),
    ) {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        count += 1;
        if count >= 1024 {
            break;
        }
    }
    if count >= 3 && max_x > min_x && max_y > min_y {
        Some((min_x, min_y, max_x - min_x, max_y - min_y))
    } else {
        None
    }
}

pub(super) fn parse_svg_shapes(svg: &str) -> Option<Vec<RawSvgShape>> {
    let bytes = svg.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut canvas: Option<(f64, f64, f64, f64)> = None;
    let mut g_stack: Vec<SvgGroupState> = Vec::new();
    let mut overflow_g_depth = 0usize;
    let mut non_render_stack: Vec<String> = Vec::new();
    let mut shapes: Vec<RawSvgShape> = Vec::new();
    let mut in_title = false;
    let mut title_buf = String::new();

    while i < len {
        if bytes[i] == b'<' {
            if svg[i..].starts_with("<!--") {
                if let Some(rel) = svg[i + 4..].find("-->") {
                    i = i + 4 + rel + 3;
                } else {
                    break;
                }
                continue;
            }
            // Any unclosed `<` to EOF terminates SVG tag parsing cleanly in O(N).
            let Some(gt) = find_html_tag_end(svg, i) else {
                break;
            };
            let tag_raw = &svg[i + 1..gt];
            let trimmed = tag_raw.trim();
            let is_closing = trimmed.starts_with('/');
            let self_closing = !is_closing && trimmed.ends_with('/');
            let name_raw = if is_closing {
                trimmed[1..].trim()
            } else {
                trimmed
            };
            let tag_name = name_raw
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();

            if is_closing {
                if non_render_stack.last().is_some_and(|top| top == &tag_name) {
                    non_render_stack.pop();
                } else if non_render_stack.is_empty() {
                    if tag_name == "g" {
                        if overflow_g_depth > 0 {
                            overflow_g_depth -= 1;
                        } else {
                            g_stack.pop();
                        }
                    } else if tag_name == "title" {
                        in_title = false;
                        if overflow_g_depth == 0 && title_buf.trim().eq_ignore_ascii_case("labels")
                        {
                            if let Some(top) = g_stack.last_mut() {
                                top.skip = true;
                            }
                        }
                        title_buf.clear();
                    }
                }
            } else if !non_render_stack.is_empty() {
                if !self_closing && non_render_stack.last().is_some_and(|top| top == &tag_name) {
                    non_render_stack.push(tag_name);
                }
            } else if is_svg_non_rendering_container(&tag_name) {
                if !self_closing {
                    non_render_stack.push(tag_name);
                }
            } else {
                match tag_name.as_str() {
                    "svg" => {
                        if canvas.is_none() {
                            canvas = parse_svg_canvas(tag_raw);
                        }
                    }
                    "title" if !self_closing => {
                        in_title = true;
                        title_buf.clear();
                    }
                    "g" if !self_closing => {
                        if g_stack.len() >= MAX_SVG_GROUP_DEPTH {
                            overflow_g_depth += 1;
                        } else {
                            let parent = g_stack.last().copied();
                            let own_fill = extract_svg_fill(tag_raw);
                            let fill = own_fill.or_else(|| parent.and_then(|p| p.fill));
                            let is_qshape =
                                parent.is_some_and(|p| p.is_qshape) || has_qshape_class(tag_raw);
                            let parent_tf = parent.map_or(IDENTITY_TRANSFORM, |p| p.transform);
                            let own_tf = extract_html_attr(tag_raw, "transform")
                                .map_or(IDENTITY_TRANSFORM, |t| parse_svg_transform(&t));
                            let transform = multiply_svg_transform(parent_tf, own_tf);
                            let skip =
                                parent.is_some_and(|p| p.skip) || is_svg_element_hidden(tag_raw);
                            g_stack.push(SvgGroupState {
                                is_qshape,
                                fill,
                                transform,
                                skip,
                            });
                        }
                    }
                    "rect" | "ellipse" | "circle" | "polygon" | "path" => {
                        let skipped = overflow_g_depth > 0
                            || g_stack.last().is_some_and(|g| g.skip)
                            || is_svg_element_hidden(tag_raw);
                        if !skipped && shapes.len() < MAX_OCCLUSION_MASKS {
                            let parent = g_stack.last().copied();
                            let effective_fill =
                                extract_svg_fill(tag_raw).or_else(|| parent.and_then(|g| g.fill));
                            if effective_fill == Some(SvgFill::None) {
                                i = gt + 1;
                                continue;
                            }
                            let geom = match tag_name.as_str() {
                                "rect" => {
                                    let x = extract_html_attr(tag_raw, "x")
                                        .and_then(|s| parse_svg_length(&s))
                                        .unwrap_or(0.0);
                                    let y = extract_html_attr(tag_raw, "y")
                                        .and_then(|s| parse_svg_length(&s))
                                        .unwrap_or(0.0);
                                    let w = extract_html_attr(tag_raw, "width")
                                        .and_then(|s| parse_svg_length(&s));
                                    let h = extract_html_attr(tag_raw, "height")
                                        .and_then(|s| parse_svg_length(&s));
                                    match (w, h) {
                                        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                                            Some((OcclusionShapeType::Rect, x, y, w, h))
                                        }
                                        _ => None,
                                    }
                                }
                                "ellipse" => {
                                    let cx = extract_html_attr(tag_raw, "cx")
                                        .and_then(|s| parse_svg_length(&s))
                                        .unwrap_or(0.0);
                                    let cy = extract_html_attr(tag_raw, "cy")
                                        .and_then(|s| parse_svg_length(&s))
                                        .unwrap_or(0.0);
                                    let rx = extract_html_attr(tag_raw, "rx")
                                        .and_then(|s| parse_svg_length(&s));
                                    let ry = extract_html_attr(tag_raw, "ry")
                                        .and_then(|s| parse_svg_length(&s));
                                    match (rx, ry) {
                                        (Some(rx), Some(ry)) if rx > 0.0 && ry > 0.0 => Some((
                                            OcclusionShapeType::Ellipse,
                                            cx - rx,
                                            cy - ry,
                                            rx * 2.0,
                                            ry * 2.0,
                                        )),
                                        _ => None,
                                    }
                                }
                                "circle" => {
                                    let cx = extract_html_attr(tag_raw, "cx")
                                        .and_then(|s| parse_svg_length(&s))
                                        .unwrap_or(0.0);
                                    let cy = extract_html_attr(tag_raw, "cy")
                                        .and_then(|s| parse_svg_length(&s))
                                        .unwrap_or(0.0);
                                    let r = extract_html_attr(tag_raw, "r")
                                        .and_then(|s| parse_svg_length(&s));
                                    match r {
                                        Some(r) if r > 0.0 => Some((
                                            OcclusionShapeType::Ellipse,
                                            cx - r,
                                            cy - r,
                                            r * 2.0,
                                            r * 2.0,
                                        )),
                                        _ => None,
                                    }
                                }
                                "polygon" => extract_html_attr(tag_raw, "points")
                                    .and_then(|p| parse_svg_polygon_bbox(&p))
                                    .map(|(x, y, w, h)| (OcclusionShapeType::Rect, x, y, w, h)),
                                "path" => extract_html_attr(tag_raw, "d")
                                    .and_then(|d| parse_svg_path_bbox(&d))
                                    .map(|(x, y, w, h)| (OcclusionShapeType::Rect, x, y, w, h)),
                                _ => None,
                            };

                            if let (Some((ox, oy, vw, vh)), Some((shape_type, rx, ry, rw, rh))) =
                                (canvas, geom)
                            {
                                let parent_tf = parent.map_or(IDENTITY_TRANSFORM, |g| g.transform);
                                let own_tf = extract_html_attr(tag_raw, "transform")
                                    .map_or(IDENTITY_TRANSFORM, |t| parse_svg_transform(&t));
                                let tf = multiply_svg_transform(parent_tf, own_tf);
                                let (tx, ty, tw, th) = transform_bbox(tf, rx, ry, rw, rh);
                                let nx = (tx - ox) / vw;
                                let ny = (ty - oy) / vh;
                                let nw = tw / vw;
                                let nh = th / vh;
                                if nx.is_finite()
                                    && ny.is_finite()
                                    && nw.is_finite()
                                    && nh.is_finite()
                                {
                                    // Clip to [0, 1] so off-canvas shapes are dropped instead of
                                    // teleported/shifted onto the image edge by `sanitize_mask`.
                                    let x1 = nx.clamp(0.0, 1.0);
                                    let y1 = ny.clamp(0.0, 1.0);
                                    let x2 = (nx + nw).clamp(0.0, 1.0);
                                    let y2 = (ny + nh).clamp(0.0, 1.0);
                                    let cw = x2 - x1;
                                    let ch = y2 - y1;
                                    if cw >= 0.002 && ch >= 0.002 {
                                        let has_qshape_class = parent.is_some_and(|g| g.is_qshape)
                                            || has_qshape_class(tag_raw);
                                        let has_active_fill =
                                            effective_fill == Some(SvgFill::Active);
                                        shapes.push(RawSvgShape {
                                            shape_type,
                                            x: x1,
                                            y: y1,
                                            w: cw,
                                            h: ch,
                                            has_qshape_class,
                                            has_active_fill,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            i = gt + 1;
            continue;
        }
        if in_title && non_render_stack.is_empty() {
            let ch = svg[i..].chars().next().unwrap();
            if title_buf.len() < 64 {
                title_buf.push(ch);
            }
            i += ch.len_utf8();
        } else {
            i += 1;
        }
    }

    canvas.map(|_| shapes)
}

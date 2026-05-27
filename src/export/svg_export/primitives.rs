//! Low-level SVG primitive emitters: `<rect>`, `<circle>`, `<polygon>`,
//! `<line>`, `<text>`, plus XML escaping.

use std::fmt::Write as _;

use image::Rgba;

fn color_hex(c: Rgba<u8>) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0[0], c.0[1], c.0[2])
}

fn opacity(c: Rgba<u8>) -> f32 {
    f32::from(c.0[3]) / 255.0
}

pub(super) fn rect(
    s: &mut String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    fill: Rgba<u8>,
    stroke: Option<Rgba<u8>>,
) {
    let _ = write!(
        s,
        r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="{f}" fill-opacity="{fo:.3}""#,
        f = color_hex(fill),
        fo = opacity(fill),
    );
    if let Some(stk) = stroke {
        let _ = write!(
            s,
            r#" stroke="{}" stroke-opacity="{:.3}""#,
            color_hex(stk),
            opacity(stk)
        );
    }
    s.push_str("/>");
}

pub(super) fn circle(
    s: &mut String,
    cx: f32,
    cy: f32,
    r: f32,
    fill: Rgba<u8>,
    stroke: Option<Rgba<u8>>,
    stroke_w: f32,
) {
    let _ = write!(
        s,
        r#"<circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" fill="{f}" fill-opacity="{fo:.3}""#,
        f = color_hex(fill),
        fo = opacity(fill),
    );
    if let Some(stk) = stroke {
        let _ = write!(
            s,
            r#" stroke="{}" stroke-opacity="{:.3}" stroke-width="{stroke_w:.2}""#,
            color_hex(stk),
            opacity(stk),
        );
    }
    s.push_str("/>");
}

pub(super) fn polygon(
    s: &mut String,
    pts: &[(f32, f32)],
    fill: Rgba<u8>,
    stroke: Option<Rgba<u8>>,
    stroke_w: f32,
) {
    s.push_str("<polygon points=\"");
    for (i, (x, y)) in pts.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        let _ = write!(s, "{x:.2},{y:.2}");
    }
    let _ = write!(
        s,
        "\" fill=\"{f}\" fill-opacity=\"{fo:.3}\"",
        f = color_hex(fill),
        fo = opacity(fill),
    );
    if let Some(stk) = stroke {
        let _ = write!(
            s,
            r#" stroke="{}" stroke-opacity="{:.3}" stroke-width="{stroke_w:.2}""#,
            color_hex(stk),
            opacity(stk),
        );
    }
    s.push_str("/>");
}

#[allow(clippy::too_many_arguments)]
pub(super) fn line(
    s: &mut String,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: Rgba<u8>,
    width: f32,
    dasharray: Option<&str>,
) {
    let _ = write!(
        s,
        r#"<line x1="{x0:.2}" y1="{y0:.2}" x2="{x1:.2}" y2="{y1:.2}" stroke="{c}" stroke-opacity="{o:.3}" stroke-width="{w:.2}" stroke-linecap="round""#,
        c = color_hex(color),
        o = opacity(color),
        w = width,
    );
    if let Some(da) = dasharray {
        let _ = write!(s, r#" stroke-dasharray="{da}""#);
    }
    s.push_str("/>");
}

pub(super) fn text(
    s: &mut String,
    x: f32,
    y: f32,
    body: &str,
    color: Rgba<u8>,
    size: f32,
    anchor: &str,
) {
    let _ = write!(
        s,
        r#"<text x="{x:.2}" y="{y:.2}" fill="{c}" fill-opacity="{o:.3}" font-size="{size:.2}" text-anchor="{anchor}">"#,
        c = color_hex(color),
        o = opacity(color),
    );
    escape_xml_into(s, body);
    s.push_str("</text>");
}

fn escape_xml_into(out: &mut String, body: &str) {
    for c in body.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
}

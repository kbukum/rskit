//! Low-level SVG element builder.

pub(crate) struct Point {
    pub x: f64,
    pub y: f64,
}

pub(crate) struct Svg {
    width: usize,
    height: usize,
    elements: Vec<String>,
}

impl Svg {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            width: w,
            height: h,
            elements: Vec::new(),
        }
    }

    pub fn rect_f(&mut self, x: f64, y: f64, w: f64, h: f64, fill: &str, attrs: &str) {
        let mut s =
            format!(r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="{fill}""#);
        if !attrs.is_empty() {
            s.push(' ');
            s.push_str(attrs);
        }
        s.push_str("/>");
        self.elements.push(s);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn line(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: &str,
        stroke_width: f64,
        attrs: &str,
    ) {
        let mut s = format!(
            r#"<line x1="{x1:.2}" y1="{y1:.2}" x2="{x2:.2}" y2="{y2:.2}" stroke="{stroke}" stroke-width="{stroke_width:.1}""#
        );
        if !attrs.is_empty() {
            s.push(' ');
            s.push_str(attrs);
        }
        s.push_str("/>");
        self.elements.push(s);
    }

    pub fn text(
        &mut self,
        x: f64,
        y: f64,
        content: &str,
        fill: &str,
        font_size: usize,
        attrs: &str,
    ) {
        let mut s = format!(r#"<text x="{x:.2}" y="{y:.2}" fill="{fill}" font-size="{font_size}""#);
        if !attrs.is_empty() {
            s.push(' ');
            s.push_str(attrs);
        }
        s.push('>');
        s.push_str(&xml_escape(content));
        s.push_str("</text>");
        self.elements.push(s);
    }

    pub fn circle(&mut self, cx: f64, cy: f64, r: f64, fill: &str, attrs: &str) {
        let mut s = format!(r#"<circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.1}" fill="{fill}""#);
        if !attrs.is_empty() {
            s.push(' ');
            s.push_str(attrs);
        }
        s.push_str("/>");
        self.elements.push(s);
    }

    pub fn polyline(
        &mut self,
        points: &[Point],
        stroke: &str,
        stroke_width: f64,
        fill: &str,
        attrs: &str,
    ) {
        let mut pts = String::new();
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                pts.push(' ');
            }
            pts.push_str(&format!("{:.2},{:.2}", p.x, p.y));
        }
        let mut s = format!(
            r#"<polyline points="{pts}" stroke="{stroke}" stroke-width="{stroke_width:.1}" fill="{fill}""#
        );
        if !attrs.is_empty() {
            s.push(' ');
            s.push_str(attrs);
        }
        s.push_str("/>");
        self.elements.push(s);
    }

    pub fn render(&self) -> String {
        let mut out = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
            self.width, self.height, self.width, self.height
        );
        out.push('\n');
        out.push_str(&format!(
            r#"<rect width="{}" height="{}" fill="white"/>"#,
            self.width, self.height
        ));
        out.push('\n');
        for el in &self.elements {
            out.push_str(el);
            out.push('\n');
        }
        out.push_str("</svg>");
        out
    }
}

pub(crate) fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Draw X and Y axes as black lines.
pub(crate) fn draw_axes(svg: &mut Svg, pad_left: usize, pad_top: usize, plot_w: f64, plot_h: f64) {
    let left = pad_left as f64;
    let top = pad_top as f64;
    svg.line(
        left,
        top + plot_h,
        left + plot_w,
        top + plot_h,
        "#333",
        1.0,
        "",
    );
    svg.line(left, top, left, top + plot_h, "#333", 1.0, "");
}

/// 8-color palette matching gokit exactly.
pub(crate) const PALETTE: &[&str] = &[
    "#4285F4", "#EA4335", "#34A853", "#FBBC05", "#9C27B0", "#FF6D00", "#00BCD4", "#795548",
];

pub(crate) fn color_at(i: usize) -> &'static str {
    PALETTE[i % PALETTE.len()]
}

/// Interpolate from light blue (#E3F2FD) at t=0 to dark blue (#1565C0) at t=1.
pub(crate) fn heat_color(t: f64) -> String {
    let t = t.clamp(0.0, 1.0);
    let r = (227.0 - t * 114.0) as u8;
    let g = (242.0 - t * 141.0) as u8;
    let b = (253.0 - t * 61.0) as u8;
    format!("#{r:02X}{g:02X}{b:02X}")
}

pub(crate) fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

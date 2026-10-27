//! Reading the vector content of a PDF page, for the take-off underlay and for importing a PDF as drawing geometry.
//!
//! A plan exported from CAD is a PDF of vector paths. This reads those paths (lines, rectangles and curves, with the
//! page's and form XObjects' transforms applied) and returns them as polylines in PDF points, origin at the lower
//! left of the page, so the engine can draw them as ordinary entities that snap and measure. Each path carries its
//! stroke and fill colours, its line width and the optional-content layer it was drawn in (the layers a CAD export
//! writes). Text shown with a font that maps its codes to characters (a simple font, or one with a ToUnicode map) is
//! returned as text runs with position, size, angle, colour and layer. Images, shadings and clipping are not read;
//! the result counts what was skipped so the caller can say so. A scanned PDF has no paths and is reported as such.
//!
//! A PDF is untrusted input: object depth, operation count, path count and point count are all capped, and every
//! number is checked for finiteness.

use std::collections::HashMap;
use std::rc::Rc;

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde_json::{Value, json};

use crate::{IoError, Result};

/// Most content operations read from one page (all its forms included).
const MAX_OPS: usize = 4_000_000;
/// Most paths and points returned.
const MAX_PATHS: usize = 200_000;
const MAX_POINTS: usize = 2_000_000;
/// Deepest form XObject nesting.
const MAX_FORM_DEPTH: usize = 8;
/// Straight segments a Bezier curve is cut into.
const CURVE_STEPS: usize = 8;
/// Largest page side accepted, in points.
const MAX_PAGE_PT: f64 = 14_400.0;
/// Most text runs, and the longest one, read from a page.
const MAX_TEXTS: usize = 100_000;
const MAX_TEXT_CHARS: usize = 2_000;
/// Most entries read from one font's ToUnicode map.
const MAX_CMAP_ENTRIES: usize = 70_000;
/// Deepest marked-content nesting followed.
const MAX_MC_DEPTH: usize = 64;

#[derive(Clone, Copy)]
struct Matrix([f64; 6]);

impl Matrix {
    const IDENTITY: Matrix = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// `self` applied first, then `then`.
    fn then(self, then: Matrix) -> Matrix {
        let (a, b) = (self.0, then.0);
        Matrix([
            a[0] * b[0] + a[1] * b[2],
            a[0] * b[1] + a[1] * b[3],
            a[2] * b[0] + a[3] * b[2],
            a[2] * b[1] + a[3] * b[3],
            a[4] * b[0] + a[5] * b[2] + b[4],
            a[4] * b[1] + a[5] * b[3] + b[5],
        ])
    }

    fn apply(self, x: f64, y: f64) -> (f64, f64) {
        let m = self.0;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    /// The mean scale of the matrix, for line widths.
    fn scale(self) -> f64 {
        let m = self.0;
        ((m[0] * m[3] - m[1] * m[2]).abs()).sqrt()
    }
}

struct Path {
    points: Vec<(f64, f64)>,
    closed: bool,
    /// Built from at least one curve segment.
    curved: bool,
}

/// One painted path.
struct Painted {
    points: Vec<(f64, f64)>,
    closed: bool,
    filled: bool,
    stroked: bool,
    curved: bool,
    rgb: [u8; 3],
    fill: [u8; 3],
    width: f64,
    layer: Option<String>,
}

/// One string shown with a font.
struct TextRun {
    text: String,
    x: f64,
    y: f64,
    /// The em size in points.
    size: f64,
    /// Radians counter-clockwise.
    angle: f64,
    rgb: [u8; 3],
    layer: Option<String>,
}

#[derive(Default)]
struct Skipped {
    /// Text shown with a font whose codes cannot be turned into characters.
    text: usize,
    images: usize,
    shadings: usize,
}

/// How a font's character codes become text.
struct Font {
    /// Code to text, from the font's ToUnicode map.
    map: HashMap<u32, String>,
    /// Two-byte codes (a composite font).
    wide: bool,
}

impl Font {
    /// The text of `bytes`; `None` when the codes cannot be read (a composite font without a map, or
    /// control characters).
    fn decode(&self, bytes: &[u8]) -> Option<String> {
        let mut out = String::new();
        if self.wide {
            if self.map.is_empty() {
                return None;
            }
            for c in bytes.chunks(2) {
                let code = c.iter().fold(0u32, |a, b| (a << 8) | u32::from(*b));
                out.push_str(self.map.get(&code)?);
            }
        } else {
            for &b in bytes {
                match self.map.get(&u32::from(b)) {
                    Some(s) => out.push_str(s),
                    // Without a map the code is taken as Latin-1, which is what the usual simple-font encodings agree on.
                    None if self.map.is_empty() => out.push(char::from(b)),
                    None => return None,
                }
            }
        }
        if out.chars().any(|c| c.is_control() && c != '\t') {
            return None;
        }
        Some(out)
    }
}

/// A PDF text string (UTF-16BE with a byte-order mark, else Latin-1).
fn pdf_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
        return String::from_utf16_lossy(&units).chars().filter(|c| !c.is_control()).take(120).collect();
    }
    bytes.iter().map(|b| char::from(*b)).filter(|c| !c.is_control()).take(120).collect()
}

/// The hex strings (`<...>`) in `s`, as numbers or UTF-16BE text.
fn hex_tokens(s: &str) -> Vec<(u32, String, usize)> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'<'
            && let Some(end) = s[i + 1..].find('>')
        {
            let hex: String = s[i + 1..i + 1 + end].chars().filter(|c| c.is_ascii_hexdigit()).collect();
            let len = hex.len() / 2;
            let value = u32::from_str_radix(&hex[..hex.len().min(8)], 16).unwrap_or(0);
            let units: Vec<u16> = (0..hex.len() / 4).filter_map(|k| u16::from_str_radix(&hex[k * 4..k * 4 + 4], 16).ok()).collect();
            out.push((value, String::from_utf16_lossy(&units), len));
            i += end + 2;
            continue;
        }
        i += 1;
    }
    out
}

/// Parse the `bfchar` and `bfrange` entries of a ToUnicode CMap.
fn parse_cmap(text: &str) -> (HashMap<u32, String>, bool) {
    let mut map = HashMap::new();
    let wide = hex_tokens(text.split("endcodespacerange").next().unwrap_or("")).first().is_some_and(|(_, _, len)| *len >= 2);
    for section in text.split("beginbfchar").skip(1) {
        let body = section.split("endbfchar").next().unwrap_or("");
        for pair in hex_tokens(body).as_chunks::<2>().0 {
            if map.len() < MAX_CMAP_ENTRIES {
                map.insert(pair[0].0, pair[1].1.clone());
            }
        }
    }
    for section in text.split("beginbfrange").skip(1) {
        let body = section.split("endbfrange").next().unwrap_or("");
        for line in body.lines() {
            let toks = hex_tokens(line);
            if toks.len() == 3 && !line.contains('[') {
                let (lo, hi) = (toks[0].0, toks[1].0.min(toks[0].0.saturating_add(0xFFFF)));
                let base: Vec<char> = toks[2].1.chars().collect();
                for (k, code) in (lo..=hi).enumerate() {
                    if map.len() >= MAX_CMAP_ENTRIES {
                        break;
                    }
                    let mut chars = base.clone();
                    if let Some(last) = chars.last_mut() {
                        *last = char::from_u32(u32::from(*last) + k as u32).unwrap_or('?');
                    }
                    map.insert(code, chars.into_iter().collect());
                }
            } else if toks.len() >= 3 && line.contains('[') {
                for (k, t) in toks[2..].iter().enumerate() {
                    if map.len() < MAX_CMAP_ENTRIES {
                        map.insert(toks[0].0.saturating_add(k as u32), t.1.clone());
                    }
                }
            }
        }
    }
    (map, wide)
}

struct Reader<'a> {
    doc: &'a Document,
    out: Vec<Painted>,
    texts: Vec<TextRun>,
    fonts: HashMap<ObjectId, Rc<Font>>,
    points: usize,
    ops: usize,
    skipped: Skipped,
    truncated: bool,
}

fn num(o: &Object) -> Option<f64> {
    let v = match o {
        Object::Integer(i) => *i as f64,
        Object::Real(r) => f64::from(*r),
        _ => return None,
    };
    v.is_finite().then_some(v)
}

fn nums(args: &[Object]) -> Option<Vec<f64>> {
    args.iter().map(num).collect()
}

fn matrix_of(o: Option<&Object>, doc: &Document) -> Matrix {
    let Some(o) = o else { return Matrix::IDENTITY };
    let Ok((_, o)) = doc.dereference(o) else { return Matrix::IDENTITY };
    let Ok(a) = o.as_array() else { return Matrix::IDENTITY };
    match nums(a) {
        Some(v) if v.len() == 6 => Matrix([v[0], v[1], v[2], v[3], v[4], v[5]]),
        _ => Matrix::IDENTITY,
    }
}

fn gray(g: f64) -> [u8; 3] {
    let v = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
    [v, v, v]
}

fn rgb(r: f64, g: f64, b: f64) -> [u8; 3] {
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [c(r), c(g), c(b)]
}

fn cmyk(c: f64, m: f64, y: f64, k: f64) -> [u8; 3] {
    let f = |v: f64| ((1.0 - v.clamp(0.0, 1.0)) * (1.0 - k.clamp(0.0, 1.0)) * 255.0).round() as u8;
    [f(c), f(m), f(y)]
}

#[derive(Clone)]
struct State {
    ctm: Matrix,
    stroke: [u8; 3],
    fill: [u8; 3],
    width: f64,
}

/// The text state of one `BT` .. `ET` run of a content stream.
struct TextState {
    tm: Matrix,
    tlm: Matrix,
    font: Option<Rc<Font>>,
    size: f64,
    leading: f64,
    /// Rendering mode 3 is invisible text (the OCR layer of a scan).
    invisible: bool,
}

impl<'a> Reader<'a> {
    /// The optional-content group name a `BDC /OC /Name` refers to.
    fn oc_name(&self, resources: Option<&Dictionary>, operands: &[Object]) -> Option<String> {
        if operands.first()?.as_name().ok()? != b"OC" {
            return None;
        }
        let prop = match operands.get(1)? {
            Object::Name(n) => {
                let props = resources?.get(b"Properties").ok().and_then(|o| self.doc.dereference(o).ok()).and_then(|(_, o)| o.as_dict().ok())?;
                self.doc.dereference(props.get(n).ok()?).ok()?.1.as_dict().ok()?
            }
            Object::Dictionary(d) => d,
            _ => return None,
        };
        match self.doc.dereference(prop.get(b"Name").ok()?).ok()?.1 {
            Object::String(bytes, _) => Some(pdf_text(bytes)).filter(|s| !s.is_empty()),
            Object::Name(n) => Some(pdf_text(n)).filter(|s| !s.is_empty()),
            _ => None,
        }
    }

    /// The font `name` of `resources`, decoded once per font object.
    fn font(&mut self, resources: Option<&Dictionary>, name: &[u8]) -> Option<Rc<Font>> {
        let fonts = resources?.get(b"Font").ok().and_then(|o| self.doc.dereference(o).ok()).and_then(|(_, o)| o.as_dict().ok())?;
        let entry = fonts.get(name).ok()?;
        let id = entry.as_reference().ok();
        if let Some(id) = id
            && let Some(f) = self.fonts.get(&id)
        {
            return Some(f.clone());
        }
        let dict = self.doc.dereference(entry).ok()?.1.as_dict().ok()?;
        let wide = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Type0");
        let (map, cmap_wide) = match dict.get(b"ToUnicode").ok().and_then(|o| self.doc.dereference(o).ok()).and_then(|(_, o)| o.as_stream().ok()) {
            Some(stream) => {
                let data = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
                parse_cmap(&String::from_utf8_lossy(&data[..data.len().min(4_000_000)]))
            }
            None => (HashMap::new(), false),
        };
        let font = Rc::new(Font { map, wide: wide || cmap_wide });
        if let Some(id) = id {
            self.fonts.insert(id, font.clone());
        }
        Some(font)
    }

    fn show(&mut self, ts: &mut TextState, st: &State, bytes: &[u8], gap_spaces: usize, layer: Option<&String>) {
        let Some(font) = ts.font.clone() else {
            self.skipped.text += 1;
            return;
        };
        if ts.invisible {
            return;
        }
        let Some(mut text) = font.decode(bytes) else {
            self.skipped.text += 1;
            return;
        };
        text.truncate(text.char_indices().nth(MAX_TEXT_CHARS).map_or(text.len(), |(i, _)| i));
        let combined = ts.tm.then(st.ctm);
        let m = combined.0;
        let size = ts.size.abs() * (m[2] * m[2] + m[3] * m[3]).sqrt();
        let (x, y) = combined.apply(0.0, 0.0);
        if !text.trim().is_empty() && size.is_finite() && size > 0.0 && x.is_finite() && y.is_finite() && self.texts.len() < MAX_TEXTS {
            self.texts.push(TextRun { text: text.clone(), x, y, size, angle: m[1].atan2(m[0]), rgb: st.fill, layer: layer.cloned() });
        }
        // Without glyph widths the pen is moved by half an em per character.
        let advance = (text.chars().count() + gap_spaces) as f64 * 0.5 * ts.size.abs();
        ts.tm = Matrix([1.0, 0.0, 0.0, 1.0, advance, 0.0]).then(ts.tm);
    }

    fn run(&mut self, content: &[u8], resources: Option<&Dictionary>, base: Matrix, depth: usize, inherited_layer: Option<String>) {
        let Ok(content) = Content::decode(content) else { return };
        let mut st = State { ctm: base, stroke: [0, 0, 0], fill: [0, 0, 0], width: 1.0 };
        let mut stack: Vec<State> = Vec::new();
        let mut paths: Vec<Path> = Vec::new();
        let mut cur: Vec<(f64, f64)> = Vec::new();
        let mut cur_curved = false;
        let mut marked: Vec<Option<String>> = Vec::new();
        let mut ts = TextState { tm: Matrix::IDENTITY, tlm: Matrix::IDENTITY, font: None, size: 0.0, leading: 0.0, invisible: false };
        for op in &content.operations {
            self.ops += 1;
            if self.ops > MAX_OPS || self.out.len() >= MAX_PATHS || self.points >= MAX_POINTS {
                self.truncated = true;
                return;
            }
            let a = &op.operands;
            let at = |ctm: Matrix, x: f64, y: f64| ctm.apply(x, y);
            match op.operator.as_str() {
                "q" => {
                    if stack.len() < 64 {
                        stack.push(st.clone());
                    }
                }
                "Q" => {
                    if let Some(s) = stack.pop() {
                        st = s;
                    }
                }
                "cm" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 6) {
                        st.ctm = Matrix([v[0], v[1], v[2], v[3], v[4], v[5]]).then(st.ctm);
                    }
                }
                "w" => {
                    if let Some(v) = a.first().and_then(num) {
                        st.width = v.abs();
                    }
                }
                "G" => {
                    if let Some(v) = a.first().and_then(num) {
                        st.stroke = gray(v);
                    }
                }
                "RG" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 3) {
                        st.stroke = rgb(v[0], v[1], v[2]);
                    }
                }
                "K" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 4) {
                        st.stroke = cmyk(v[0], v[1], v[2], v[3]);
                    }
                }
                "BDC" | "BMC" => {
                    if marked.len() < MAX_MC_DEPTH {
                        let name = if op.operator == "BDC" { self.oc_name(resources, a) } else { None };
                        marked.push(name);
                    }
                }
                "EMC" => {
                    marked.pop();
                }
                "g" => {
                    if let Some(v) = a.first().and_then(num) {
                        st.fill = gray(v);
                    }
                }
                "rg" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 3) {
                        st.fill = rgb(v[0], v[1], v[2]);
                    }
                }
                "k" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 4) {
                        st.fill = cmyk(v[0], v[1], v[2], v[3]);
                    }
                }
                "BT" => {
                    ts.tm = Matrix::IDENTITY;
                    ts.tlm = Matrix::IDENTITY;
                }
                "Tf" => {
                    if let Some(name) = a.first().and_then(|o| o.as_name().ok()) {
                        ts.font = self.font(resources, name);
                        ts.size = a.get(1).and_then(num).unwrap_or(0.0);
                    }
                }
                "TL" => {
                    if let Some(v) = a.first().and_then(num) {
                        ts.leading = v;
                    }
                }
                "Tr" => {
                    ts.invisible = a.first().and_then(num).is_some_and(|m| m == 3.0);
                }
                "Td" | "TD" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 2) {
                        if op.operator == "TD" {
                            ts.leading = -v[1];
                        }
                        ts.tlm = Matrix([1.0, 0.0, 0.0, 1.0, v[0], v[1]]).then(ts.tlm);
                        ts.tm = ts.tlm;
                    }
                }
                "Tm" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 6) {
                        ts.tlm = Matrix([v[0], v[1], v[2], v[3], v[4], v[5]]);
                        ts.tm = ts.tlm;
                    }
                }
                "T*" => {
                    ts.tlm = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, -ts.leading]).then(ts.tlm);
                    ts.tm = ts.tlm;
                }
                "Tj" | "'" | "\"" => {
                    if matches!(op.operator.as_str(), "'" | "\"") {
                        ts.tlm = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, -ts.leading]).then(ts.tlm);
                        ts.tm = ts.tlm;
                    }
                    let layer = marked.iter().rev().find_map(Option::as_ref).or(inherited_layer.as_ref()).cloned();
                    if let Some(Object::String(bytes, _)) = a.last() {
                        self.show(&mut ts, &st, bytes, 0, layer.as_ref());
                    }
                }
                "TJ" => {
                    let layer = marked.iter().rev().find_map(Option::as_ref).or(inherited_layer.as_ref()).cloned();
                    let mut bytes: Vec<u8> = Vec::new();
                    if let Some(Object::Array(items)) = a.first() {
                        for item in items.iter().take(10_000) {
                            match item {
                                Object::String(s, _) => bytes.extend_from_slice(s),
                                // A large backwards move between pieces is a word space.
                                o => {
                                    if num(o).is_some_and(|k| k < -250.0) {
                                        bytes.push(b' ');
                                    }
                                }
                            }
                        }
                    }
                    if !bytes.is_empty() {
                        self.show(&mut ts, &st, &bytes, 0, layer.as_ref());
                    }
                }
                "m" => {
                    flush(&mut paths, &mut cur, false, &mut cur_curved);
                    if let Some(v) = nums(a).filter(|v| v.len() == 2) {
                        cur.push(at(st.ctm, v[0], v[1]));
                    }
                }
                "l" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 2) {
                        cur.push(at(st.ctm, v[0], v[1]));
                    }
                }
                "c" | "v" | "y" => {
                    let Some(v) = nums(a) else { continue };
                    let Some(&p0) = cur.last() else { continue };
                    cur_curved = true;
                    let pt = |x: f64, y: f64| at(st.ctm, x, y);
                    let (p1, p2, p3) = match (op.operator.as_str(), v.len()) {
                        ("c", 6) => (pt(v[0], v[1]), pt(v[2], v[3]), pt(v[4], v[5])),
                        ("v", 4) => (p0, pt(v[0], v[1]), pt(v[2], v[3])),
                        ("y", 4) => (pt(v[0], v[1]), pt(v[2], v[3]), pt(v[2], v[3])),
                        _ => continue,
                    };
                    for i in 1..=CURVE_STEPS {
                        let t = i as f64 / CURVE_STEPS as f64;
                        let u = 1.0 - t;
                        let x = u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
                        let y = u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
                        cur.push((x, y));
                    }
                }
                "re" => {
                    flush(&mut paths, &mut cur, false, &mut cur_curved);
                    if let Some(v) = nums(a).filter(|v| v.len() == 4) {
                        let (x, y, w, h) = (v[0], v[1], v[2], v[3]);
                        let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)].map(|(px, py)| at(st.ctm, px, py));
                        paths.push(Path { points: pts.to_vec(), closed: true, curved: false });
                    }
                }
                "h" => {
                    if !cur.is_empty() {
                        flush(&mut paths, &mut cur, true, &mut cur_curved);
                    }
                }
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                    let close = matches!(op.operator.as_str(), "s" | "b" | "b*");
                    flush(&mut paths, &mut cur, close, &mut cur_curved);
                    let (stroked, filled) = match op.operator.as_str() {
                        "S" | "s" => (true, false),
                        "f" | "F" | "f*" => (false, true),
                        "B" | "B*" | "b" | "b*" => (true, true),
                        _ => (false, false),
                    };
                    if stroked || filled {
                        let layer = marked.iter().rev().find_map(Option::as_ref).or(inherited_layer.as_ref()).cloned();
                        for p in paths.drain(..) {
                            if p.points.len() < 2 || self.points + p.points.len() > MAX_POINTS {
                                continue;
                            }
                            self.points += p.points.len();
                            self.out.push(Painted {
                                points: p.points,
                                closed: p.closed,
                                filled,
                                stroked,
                                curved: p.curved,
                                rgb: st.stroke,
                                fill: st.fill,
                                width: st.width * st.ctm.scale(),
                                layer: layer.clone(),
                            });
                        }
                    } else {
                        paths.clear();
                    }
                }
                "BI" => self.skipped.images += 1,
                "sh" => self.skipped.shadings += 1,
                "Do" => {
                    let Some(name) = a.first().and_then(|o| o.as_name().ok()) else { continue };
                    let layer = marked.iter().rev().find_map(Option::as_ref).or(inherited_layer.as_ref()).cloned();
                    self.xobject(resources, name, st.ctm, depth, layer);
                }
                _ => {}
            }
        }
    }

    fn xobject(&mut self, resources: Option<&Dictionary>, name: &[u8], ctm: Matrix, depth: usize, layer: Option<String>) {
        let Some(res) = resources else { return };
        let Some(xo) = res.get(b"XObject").ok().and_then(|o| self.doc.dereference(o).ok()).and_then(|(_, o)| o.as_dict().ok()) else { return };
        let Some(entry) = xo.get(name).ok() else { return };
        let Ok((_, obj)) = self.doc.dereference(entry) else { return };
        let Ok(stream) = obj.as_stream() else { return };
        match stream.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) {
            Some(b"Image") => self.skipped.images += 1,
            Some(b"Form") if depth < MAX_FORM_DEPTH => {
                let Ok(data) = stream.decompressed_content() else { return };
                let m = matrix_of(stream.dict.get(b"Matrix").ok(), self.doc).then(ctm);
                let own = stream.dict.get(b"Resources").ok().and_then(|o| self.doc.dereference(o).ok()).and_then(|(_, o)| o.as_dict().ok());
                self.run(&data, own.or(resources), m, depth + 1, layer);
            }
            _ => {}
        }
    }
}

fn flush(paths: &mut Vec<Path>, cur: &mut Vec<(f64, f64)>, closed: bool, curved: &mut bool) {
    if cur.len() >= 2 {
        paths.push(Path { points: std::mem::take(cur), closed, curved: *curved });
    } else {
        cur.clear();
    }
    *curved = false;
}

/// The page box (inherited through `Parent`), its rotation, in points.
fn page_box(doc: &Document, page: ObjectId) -> Option<([f64; 4], i64)> {
    let mut id = page;
    let mut rotate = None;
    for _ in 0..32 {
        let d = doc.get_dictionary(id).ok()?;
        if rotate.is_none() {
            rotate = d.get(b"Rotate").ok().and_then(|o| doc.dereference(o).ok()).and_then(|(_, o)| o.as_i64().ok());
        }
        if let Some(b) = d.get(b"MediaBox").ok().and_then(|o| doc.dereference(o).ok()).and_then(|(_, o)| o.as_array().ok()) {
            let v = nums(b).filter(|v| v.len() == 4)?;
            return Some(([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])], rotate.unwrap_or(0)));
        }
        id = d.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

/// Read page `page` (from 1) of a PDF. The result is JSON:
/// `{pages, width_pt, height_pt, paths: [{pts: [[x, y]...], closed, filled, stroked, curved, rgb, fill_rgb, width_pt, layer}],
/// texts: [{text, x, y, size_pt, angle, rgb, layer}], skipped: {text, images, shadings}, truncated}`.
pub fn read_page(bytes: &[u8], page: u32) -> Result<Value> {
    let doc = Document::load_mem(bytes).map_err(|e| IoError::Format(format!("not a readable PDF: {e}")))?;
    if doc.is_encrypted() {
        return Err(IoError::Format("the PDF is encrypted; save an unprotected copy first".into()));
    }
    let pages = doc.get_pages();
    let id = *pages.get(&page).ok_or_else(|| IoError::Format(format!("the PDF has {} page(s); there is no page {page}", pages.len())))?;
    let ([x0, y0, x1, y1], rotate) = page_box(&doc, id).ok_or_else(|| IoError::Format("the page has no usable MediaBox".into()))?;
    let (w, h) = (x1 - x0, y1 - y0);
    if !(w > 0.0 && h > 0.0 && w <= MAX_PAGE_PT && h <= MAX_PAGE_PT) {
        return Err(IoError::Format("the page size is out of range".into()));
    }
    // Origin at the lower left of the page box; a page rotation turns the page the way a viewer shows it.
    let origin = Matrix([1.0, 0.0, 0.0, 1.0, -x0, -y0]);
    let (base, width, height) = match rotate.rem_euclid(360) {
        90 => (origin.then(Matrix([0.0, -1.0, 1.0, 0.0, 0.0, w])), h, w),
        180 => (origin.then(Matrix([-1.0, 0.0, 0.0, -1.0, w, h])), w, h),
        270 => (origin.then(Matrix([0.0, 1.0, -1.0, 0.0, h, 0.0])), h, w),
        _ => (origin, w, h),
    };
    let content = doc.get_page_content(id);
    let resources = doc.get_page_resources(id).ok().and_then(|(r, _)| r);
    let mut reader = Reader {
        doc: &doc,
        out: Vec::new(),
        texts: Vec::new(),
        fonts: HashMap::new(),
        points: 0,
        ops: 0,
        skipped: Skipped::default(),
        truncated: false,
    };
    reader.run(&content, resources, base, 0, None);
    let paths: Vec<Value> = reader
        .out
        .iter()
        .map(|p| {
            let pts: Vec<Value> = p.points.iter().map(|(x, y)| json!([x, y])).collect();
            json!({
                "pts": pts, "closed": p.closed, "filled": p.filled, "stroked": p.stroked, "curved": p.curved,
                "rgb": p.rgb, "fill_rgb": p.fill, "width_pt": p.width, "layer": p.layer,
            })
        })
        .collect();
    let texts: Vec<Value> = reader
        .texts
        .iter()
        .map(|t| json!({ "text": t.text, "x": t.x, "y": t.y, "size_pt": t.size, "angle": t.angle, "rgb": t.rgb, "layer": t.layer }))
        .collect();
    Ok(json!({
        "pages": pages.len(), "width_pt": width, "height_pt": height, "paths": paths, "texts": texts,
        "skipped": { "text": reader.skipped.text, "images": reader.skipped.images, "shadings": reader.skipped.shadings },
        "truncated": reader.truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-page PDF made by hand: a stroked line, a rectangle and a filled triangle under a translation.
    fn sample() -> Vec<u8> {
        let content = "0 0 1 RG 2 w 10 10 m 110 10 l S\n20 20 50 30 re S\nq 1 0 0 1 100 100 cm 0 0 m 40 0 l 20 30 l f Q\nBT (hello) Tj ET";
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 150] /Contents 4 0 R /Resources << >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len() + 1),
        ];
        for (i, o) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
        }
        let xref = pdf.len();
        pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1));
        for o in &offsets {
            pdf.push_str(&format!("{o:010} 00000 n \n"));
        }
        pdf.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1));
        pdf.into_bytes()
    }

    #[test]
    fn a_hand_made_page_gives_its_paths_in_points() {
        let r = read_page(&sample(), 1).unwrap();
        assert_eq!((r["width_pt"].as_f64(), r["height_pt"].as_f64(), r["pages"].as_u64()), (Some(200.0), Some(150.0), Some(1)));
        let paths = r["paths"].as_array().unwrap();
        assert_eq!(paths.len(), 3, "{r}");
        assert_eq!(paths[0]["pts"], json!([[10.0, 10.0], [110.0, 10.0]]));
        assert_eq!(paths[0]["rgb"], json!([0, 0, 255]));
        assert_eq!(paths[1]["closed"], true);
        assert_eq!(paths[1]["pts"].as_array().unwrap().len(), 4);
        // The triangle sits under `1 0 0 1 100 100 cm`.
        assert_eq!(paths[2]["pts"][0], json!([100.0, 100.0]));
        assert_eq!(paths[2]["filled"], true);
        assert_eq!(r["skipped"]["text"], 1, "the text is counted, not drawn");
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert!(read_page(b"", 1).is_err());
        assert!(read_page(b"%PDF-1.4 nonsense", 1).is_err());
        assert!(read_page(&sample(), 2).is_err(), "no page 2");
        assert!(read_page(&sample(), 0).is_err());
        let mut cut = sample();
        cut.truncate(cut.len() / 2);
        let _ = read_page(&cut, 1);
    }

    /// A PDF whose form XObject draws itself: the nesting cap ends it, quickly, with what was drawn on the way.
    #[test]
    fn a_form_that_draws_itself_ends() {
        let page = "/Fm0 Do 0 0 m 10 10 l S";
        let form = "0 0 m 5 5 l S /Fm0 Do";
        let res = "<< /XObject << /Fm0 5 0 R >> >>";
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R /Resources {res} >>"),
            format!("<< /Length {} >>\nstream\n{page}\nendstream", page.len() + 1),
            format!("<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Resources {res} /Length {} >>\nstream\n{form}\nendstream", form.len() + 1),
        ];
        for (i, o) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
        }
        let xref = pdf.len();
        pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1));
        for o in &offsets {
            pdf.push_str(&format!("{o:010} 00000 n \n"));
        }
        pdf.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1));
        let r = read_page(pdf.as_bytes(), 1).unwrap();
        let n = r["paths"].as_array().unwrap().len();
        assert!((2..=20).contains(&n), "the recursion is cut at the depth cap: {n} paths");
    }

    /// A one-page PDF with the given content and extra objects (numbered from 5); resources are `res`.
    fn pdf_with(content: &str, res: &str, extra: &[String]) -> Vec<u8> {
        let mut pdf = String::from(
            "%PDF-1.5
",
        );
        let mut offsets = Vec::new();
        let mut objs = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 150] /Contents 4 0 R /Resources {res} >>"),
            format!(
                "<< /Length {} >>
stream
{content}
endstream",
                content.len() + 1
            ),
        ];
        objs.extend(extra.iter().cloned());
        for (i, o) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!(
                "{} 0 obj
{o}
endobj
",
                i + 1
            ));
        }
        let xref = pdf.len();
        pdf.push_str(&format!(
            "xref
0 {}
0000000000 65535 f 
",
            objs.len() + 1
        ));
        for o in &offsets {
            pdf.push_str(&format!(
                "{o:010} 00000 n 
"
            ));
        }
        pdf.push_str(&format!(
            "trailer
<< /Size {} /Root 1 0 R >>
startxref
{xref}
%%EOF
",
            objs.len() + 1
        ));
        pdf.into_bytes()
    }

    #[test]
    fn paths_carry_their_colours_and_optional_content_layer() {
        let content = "/OC /L0 BDC 1 0 0 RG 0 0 m 100 0 l S EMC 0 1 0 rg 10 10 20 20 re f /OC /L1 BDC 0 0 1 RG 5 5 m 50 50 l S EMC";
        let res = "<< /Properties << /L0 5 0 R /L1 6 0 R >> >>";
        let bytes = pdf_with(content, res, &["<< /Type /OCG /Name (A-WALL) >>".into(), "<< /Type /OCG /Name <FEFF00450041> >>".into()]);
        let v = read_page(&bytes, 1).unwrap();
        let paths = v["paths"].as_array().unwrap();
        assert_eq!(paths.len(), 3);
        assert_eq!((paths[0]["layer"].as_str(), paths[0]["rgb"].clone()), (Some("A-WALL"), json!([255, 0, 0])));
        assert_eq!(paths[1]["layer"], Value::Null, "outside marked content: no layer");
        assert_eq!((paths[1]["filled"].clone(), paths[1]["fill_rgb"].clone()), (json!(true), json!([0, 255, 0])), "a fill takes the fill colour");
        assert_eq!(paths[2]["layer"].as_str(), Some("EA"), "UTF-16 names are read");
    }

    #[test]
    fn text_is_read_with_its_position_size_angle_and_layer() {
        let content = "/OC /L0 BDC BT /F1 12 Tf 1 0 0 rg 50 60 Td (Room 1) Tj ET EMC BT /F1 10 Tf 0 1 -1 0 100 20 Tm [(A)-400(B)] TJ ET BT 3 Tr /F1 12 Tf 5 5 Td (hidden) Tj ET";
        let res = "<< /Properties << /L0 5 0 R >> /Font << /F1 6 0 R >> >>";
        let bytes =
            pdf_with(content, res, &["<< /Type /OCG /Name (A-TEXT) >>".into(), "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into()]);
        let v = read_page(&bytes, 1).unwrap();
        let texts = v["texts"].as_array().unwrap();
        assert_eq!(texts.len(), 2, "invisible text is not drawn: {texts:?}");
        assert_eq!(texts[0]["text"], "Room 1");
        assert_eq!((texts[0]["x"].clone(), texts[0]["y"].clone(), texts[0]["size_pt"].clone()), (json!(50.0), json!(60.0), json!(12.0)));
        assert_eq!((texts[0]["layer"].as_str(), texts[0]["rgb"].clone()), (Some("A-TEXT"), json!([255, 0, 0])));
        assert_eq!(texts[1]["text"], "A B", "a large kerning gap is a word space");
        assert!((texts[1]["angle"].as_f64().unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "a rotated text matrix gives the angle");
    }

    #[test]
    fn a_composite_font_is_read_through_its_tounicode_map_and_without_one_it_is_skipped() {
        let cmap = "/CIDInit /ProcSet findresource begin begincmap 1 begincodespacerange <0000> <FFFF> endcodespacerange                     2 beginbfchar <0001> <0048> <0002> <0069> endbfchar 1 beginbfrange <0003> <0005> <0041> endbfrange endcmap";
        let stream = format!(
            "<< /Length {} >>
stream
{cmap}
endstream",
            cmap.len() + 1
        );
        let fonts = [
            "<< /Type /Font /Subtype /Type0 /BaseFont /X /Encoding /Identity-H /ToUnicode 6 0 R >>".to_string(),
            stream,
            "<< /Type /Font /Subtype /Type0 /BaseFont /Y /Encoding /Identity-H >>".to_string(),
        ];
        let content = "BT /F1 10 Tf 10 10 Td <00010002> Tj ET BT /F1 10 Tf 10 30 Td <000300040005> Tj ET BT /F2 10 Tf 10 50 Td <0001> Tj ET";
        let res = "<< /Font << /F1 5 0 R /F2 7 0 R >> >>";
        let v = read_page(&pdf_with(content, res, &fonts), 1).unwrap();
        let texts: Vec<&str> = v["texts"].as_array().unwrap().iter().filter_map(|t| t["text"].as_str()).collect();
        assert_eq!(texts, ["Hi", "ABC"], "bfchar and bfrange");
        assert_eq!(v["skipped"]["text"], 1, "no map: not guessed");
    }

    #[test]
    fn hostile_text_and_marked_content_do_not_panic() {
        let mut content = String::new();
        for _ in 0..200 {
            content.push_str("/OC /Nope BDC BMC ");
        }
        content.push_str("EMC EMC EMC BT /F1 -0 Tf (x) Tj 1e308 0 0 1e308 0 0 Tm (y) Tj ET BT <FFFF> Tj [(a) 1e999] TJ ET");
        let res = "<< /Font << /F1 5 0 R >> /Properties 99 0 R >>";
        let bytes = pdf_with(&content, res, &["<< /Type /Font /Subtype /Type1 /ToUnicode 5 0 R >>".into()]);
        let v = read_page(&bytes, 1).unwrap();
        assert!(v["texts"].as_array().unwrap().len() <= 3);
        for cmap in ["<", "begincodespacerange", "beginbfrange <00> <FFFFFFFF> <0041> endbfrange", "beginbfchar <FFFFFFFFFF> <D800> endbfchar"] {
            let _ = parse_cmap(cmap);
        }
    }
}

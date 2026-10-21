//! The display list the layout fills, and the PDF 1.7 file written from it.
//!
//! The file is written by hand, as the Factur-X writer in khata-en16931
//! is: a catalog, a page tree, an information dictionary, the fonts, image
//! XObjects and a compressed content stream per page; then a
//! cross-reference table with 20-byte entries and a trailer with an `/ID`.
//! Nothing depends on the clock or on the order fonts were loaded in, so
//! the same invoice gives the same bytes.
//!
//! Each font used is embedded as a subset (`subsetter`) in a Type0 font
//! with `/Identity-H` two-byte codes:
//!
//! - TrueType outlines: a CIDFontType2. Codes are given out per glyph *and*
//!   the text it stands for, and a `/CIDToGIDMap` stream maps them to the
//!   subset's glyphs, so a glyph shared by different clusters (a Devanagari
//!   i-matra in कि and in मि) still copies as each.
//! - CFF outlines (system CJK fonts): a CIDFontType0 with the subset's CFF
//!   program (`/FontFile3 /CIDFontType0C`); the code is the subset's glyph.
//!
//! The `/ToUnicode` map gives each code its share of its cluster's text,
//! so the text can be copied and searched. Glyphs that stand for no text
//! (an Arabic letter's separate dots, a mark whose cluster's letters went
//! to the other glyphs) are drawn after the line's other glyphs, so readers
//! see the text glyphs edge to edge, and get codes that are control or
//! invisible characters, for readers that show an unmapped code as itself.
//! Checked with poppler (`pdftotext`), Apple PDFKit and MuPDF. Glyphs are placed with `TJ` adjustments, and a
//! new text matrix where a mark sits higher or lower.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io::Write as _;

use flate2::Compression;
use flate2::write::ZlibEncoder;

use super::font::{self, Font, FontId};
use super::image::PdfImage;
use super::shape::Glyph;

pub(crate) type Rgb = (f32, f32, f32);

/// Something drawn on a page. Coordinates are points from the page's top
/// left corner (the writer turns them round); text `x` is its left edge,
/// `y` the baseline, `w` its width.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Op {
    Text { x: f32, y: f32, w: f32, size: f32, bold: bool, colour: Rgb, glyphs: Vec<Glyph> },
    Rect { x: f32, y: f32, w: f32, h: f32, fill: Option<Rgb>, stroke: Option<(Rgb, f32)> },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, colour: Rgb, width: f32 },
    Image { id: usize, x: f32, y: f32, w: f32, h: f32 },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Page {
    pub w: f32,
    pub h: f32,
    pub ops: Vec<Op>,
}

/// Everything the writer needs.
#[derive(Debug, Default)]
pub(crate) struct Display {
    pub pages: Vec<Page>,
    pub images: Vec<PdfImage>,
    pub title: String,
}

/// A number for the content stream: at most 3 decimals, no trailing zeros.
fn n(v: f32) -> String {
    let s = format!("{:.3}", if v.abs() < 0.0005 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_owned() }
}

fn colour(c: Rgb, op: &str) -> String {
    format!("{} {} {} {op}", n(c.0), n(c.1), n(c.2))
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).expect("writing to memory");
    e.finish().expect("writing to memory")
}

/// A font as this file uses it: the codes given out and what they show.
struct FontUse {
    font: &'static Font,
    /// `/F1`, `/F2`… in order of first use.
    resource: usize,
    /// (glyph, text) of each code given out; code 0 is .notdef.
    codes: Vec<Option<(u16, String)>>,
    by_glyph: HashMap<(u16, String), u16>,
    /// Codes left for marks that stand for no text.
    silent: Vec<u16>,
    /// Where to look for the next code for a glyph with text.
    next: usize,
    /// CFF: the subset's glyph ids, decided before codes are given out.
    remap: Option<subsetter::GlyphRemapper>,
}

impl FontUse {
    fn code(&mut self, g: &Glyph) -> u16 {
        if let Some(r) = &self.remap {
            // The code is the subset's glyph id; the first text wins.
            let c = r.get(g.gid).unwrap_or(0);
            let slot = self.codes[usize::from(c)].get_or_insert((g.gid, String::new()));
            if slot.1.is_empty() && !g.text.is_empty() {
                slot.1 = g.text.clone();
            }
            return c;
        }
        let key = (g.gid, g.text.clone());
        if let Some(&c) = self.by_glyph.get(&key) {
            return c;
        }
        // A reader that finds no ToUnicode text for a code may print the
        // code itself as a character, so marks with no text of their own get
        // codes that are control characters there; the rest start at U+0100.
        let c = match (key.1.is_empty(), self.silent.pop()) {
            (true, Some(c)) => {
                if self.codes.len() <= usize::from(c) {
                    self.codes.resize(usize::from(c) + 1, None);
                }
                c
            }
            (silent, spare) => {
                if let Some(c) = spare.filter(|_| !silent) {
                    self.silent.push(c);
                }
                // The next free code from U+0100, around the silent ones.
                while self.codes.get(self.next).is_some_and(Option::is_some) || self.silent.contains(&u16::try_from(self.next).unwrap_or(0)) {
                    self.next += 1;
                }
                let c = self.next;
                if self.codes.len() <= c {
                    self.codes.resize(c + 1, None);
                }
                u16::try_from(c).unwrap_or(0)
            }
        };
        self.codes[usize::from(c)] = Some(key.clone());
        self.by_glyph.insert(key, c);
        c
    }

    fn new(font: &'static Font, resource: usize) -> FontUse {
        let mut codes = vec![None; 0x100];
        codes[0] = Some((0, String::new()));
        // C0 and C1 controls but tab, line feed, carriage return and the
        // like, then invisible (default-ignorable) characters.
        let mut silent: Vec<u16> = (0x01..=0x08)
            .chain(0x0E..=0x1F)
            .chain(0x7F..=0x9F)
            .chain([0x034F, 0x115F, 0x1160, 0x17B4, 0x17B5, 0x3164, 0xFFA0])
            .chain(0x180B..=0x180F)
            .chain(0x200B..=0x200F)
            .chain(0x2060..=0x2064)
            .chain(0x206A..=0x206F)
            .chain(0xFE00..=0xFE0F)
            .chain(0xFFF0..=0xFFF8)
            .collect();
        silent.reverse();
        FontUse { font, resource, codes, by_glyph: HashMap::new(), silent, next: 0x100, remap: None }
    }

    /// The glyph a code shows.
    fn glyph(&self, code: u16) -> u16 {
        self.codes.get(usize::from(code)).and_then(|c| c.as_ref()).map_or(0, |c| c.0)
    }
}

/// The fonts of a file, in order of first use, with their codes.
struct Fonts {
    uses: Vec<FontUse>,
    by_id: HashMap<FontId, usize>,
}

impl Fonts {
    fn new(d: &Display) -> Fonts {
        let mut f = Fonts { uses: Vec::new(), by_id: HashMap::new() };
        let mut cff_glyphs: BTreeMap<usize, BTreeSet<u16>> = BTreeMap::new();
        for p in &d.pages {
            for op in &p.ops {
                if let Op::Text { glyphs, .. } = op {
                    for g in glyphs {
                        let i = *f.by_id.entry(g.font).or_insert_with(|| {
                            let font = font::get(g.font);
                            f.uses.push(FontUse::new(font, f.uses.len() + 1));
                            f.uses.len() - 1
                        });
                        if f.uses[i].font.cff {
                            cff_glyphs.entry(i).or_default().insert(g.gid);
                        }
                    }
                }
            }
        }
        for (i, gids) in cff_glyphs {
            let gids: Vec<u16> = gids.into_iter().collect();
            let r = subsetter::GlyphRemapper::new_from_glyphs_sorted(&gids);
            f.uses[i].codes = vec![None; usize::from(r.num_gids())];
            f.uses[i].codes[0] = Some((0, String::new()));
            for &g in &gids {
                if let Some(c) = r.get(g) {
                    f.uses[i].codes[usize::from(c)] = Some((g, String::new()));
                }
            }
            f.uses[i].remap = Some(r);
        }
        f
    }

    fn of(&mut self, id: FontId) -> &mut FontUse {
        let i = self.by_id[&id];
        &mut self.uses[i]
    }
}

/// A text run: glyphs grouped into `TJ` arrays of one font and height.
fn text(s: &mut String, fonts: &mut Fonts, x: f32, baseline: f32, size: f32, glyphs: &[Glyph]) {
    // Each glyph's place, in thousandths of the size from (x, baseline).
    let mut pen = 0.0f32;
    let mut cur: Option<(FontId, f32, bool)> = None;
    let mut cur_font: Option<FontId> = None;
    // Where the previous glyph left the text position, in thousandths.
    let mut at = 0.0f32;
    let mut open = false;
    let k = size / 1000.0;
    // Each glyph at its place; marks that stand for no text and don't
    // advance (the text went to their cluster's other glyphs) come last, so
    // readers see the line's text glyphs edge to edge: a mark among them,
    // its origin moved left or right, reads as a gap and so a space.
    let mut placed: Vec<(f32, &Glyph)> = Vec::with_capacity(glyphs.len());
    for g in glyphs {
        placed.push((pen, g));
        pen += g.advance;
    }
    let silent = |g: &Glyph| g.text.is_empty() && g.advance == 0.0;
    placed.sort_by_key(|(_, g)| silent(g));
    for &(pen, g) in &placed {
        let gx = pen + g.dx;
        let gy = g.dy;
        let fu = fonts.of(g.font);
        let code = fu.code(g);
        let width = fu.font.advance_1000(fu.glyph(code)) as f32;
        let resource = fu.resource;
        if cur != Some((g.font, gy, silent(g))) {
            if open {
                s.push_str("] TJ\n");
                open = false;
            }
            if cur_font != Some(g.font) {
                let _ = write!(s, "/F{resource} {} Tf ", n(size));
                cur_font = Some(g.font);
            }
            let _ = write!(s, "1 0 0 1 {} {} Tm ", n(x + gx * k), n(baseline + gy * k));
            cur = Some((g.font, gy, silent(g)));
            at = gx;
        }
        if !open {
            s.push('[');
            open = true;
        }
        let adjust = at - gx;
        if adjust.abs() > 0.05 {
            s.push_str(&n(adjust));
        }
        let _ = write!(s, "<{code:04X}>");
        at = gx + width;
    }
    if open {
        s.push_str("] TJ\n");
    }
}

fn content(page: &Page, fonts: &mut Fonts) -> Vec<u8> {
    let mut s = String::new();
    let flip = |y: f32| page.h - y;
    for op in &page.ops {
        match op {
            Op::Text { x, y, size, bold, colour: c, glyphs, .. } => {
                if glyphs.is_empty() {
                    continue;
                }
                let bold = if *bold { format!("2 Tr {} w {} ", n(size * 0.035), colour(*c, "RG")) } else { String::new() };
                let _ = writeln!(s, "q BT {} {bold}", colour(*c, "rg"));
                text(&mut s, fonts, *x, flip(*y), *size, glyphs);
                s.push_str("ET Q\n");
            }
            Op::Rect { x, y, w, h, fill, stroke } => {
                let mut p = String::from("q ");
                if let Some(f) = fill {
                    p.push_str(&colour(*f, "rg"));
                    p.push(' ');
                }
                if let Some((c, lw)) = stroke {
                    let _ = write!(p, "{} {} w ", colour(*c, "RG"), n(*lw));
                }
                let paint = match (fill.is_some(), stroke.is_some()) {
                    (true, true) => "B",
                    (true, false) => "f",
                    (false, true) => "S",
                    (false, false) => "n",
                };
                let _ = writeln!(s, "{p}{} {} {} {} re {paint} Q", n(*x), n(flip(y + h)), n(*w), n(*h));
            }
            Op::Line { x1, y1, x2, y2, colour: c, width } => {
                let _ = writeln!(s, "q {} {} w {} {} m {} {} l S Q", colour(*c, "RG"), n(*width), n(*x1), n(flip(*y1)), n(*x2), n(flip(*y2)));
            }
            Op::Image { id, x, y, w, h } => {
                let _ = writeln!(s, "q {} 0 0 {} {} {} cm /Im{id} Do Q", n(*w), n(*h), n(*x), n(flip(y + h)));
            }
        }
    }
    s.into_bytes()
}

/// A text string as UTF-16BE hex with a byte order mark.
fn text_string(s: &str) -> String {
    let mut out = String::from("<FEFF");
    for u in s.encode_utf16() {
        let _ = write!(out, "{u:04X}");
    }
    out.push('>');
    out
}

/// 128 bits from two FNV-1a hashes: the file `/ID` and the subset tags.
fn digest(data: &[u8]) -> [u64; 2] {
    let mut a: u64 = 0xcbf2_9ce4_8422_2325;
    let mut b: u64 = 0x8422_2325_cbf2_9ce4;
    for &x in data {
        a = (a ^ u64::from(x)).wrapping_mul(0x0100_0000_01b3);
        b = (b ^ u64::from(x)).wrapping_mul(0x0000_0100_0000_01b3).rotate_left(5);
    }
    [a, b]
}

struct Objects {
    out: Vec<u8>,
    offsets: Vec<usize>,
}

impl Objects {
    fn object(&mut self, id: usize, body: &[u8]) {
        if self.offsets.len() <= id {
            self.offsets.resize(id + 1, 0);
        }
        self.offsets[id] = self.out.len();
        let _ = writeln!(self.out, "{id} 0 obj");
        self.out.extend_from_slice(body);
        self.out.extend_from_slice(b"\nendobj\n");
    }

    fn stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        let mut body = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        self.object(id, &body);
    }
}

const CATALOG: usize = 1;
const PAGES: usize = 2;
const INFO: usize = 3;
const FIRST_FREE: usize = 4;
/// Objects per font: Type0, CIDFont, descriptor, program, ToUnicode, CIDToGIDMap.
const FONT_OBJECTS: usize = 6;

fn to_unicode_cmap(codes: &[Option<(u16, String)>]) -> String {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    // Marks whose cluster's text went to another glyph map to nothing (an
    // empty string), so a reader doesn't take the code itself for a character.
    let entries: Vec<(usize, &String)> = codes.iter().enumerate().skip(1).filter_map(|(c, x)| x.as_ref().map(|(_, t)| (c, t))).collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (c, t) in chunk {
            let hex: String = t.encode_utf16().map(|u| format!("{u:04X}")).collect();
            let _ = writeln!(s, "<{c:04X}> <{hex}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

/// The table `tag` of an OpenType font.
fn table<'a>(font: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let count = usize::from(u16::from_be_bytes(font.get(4..6)?.try_into().ok()?));
    (0..count).find_map(|i| {
        let r = font.get(12 + 16 * i..28 + 16 * i)?;
        if &r[..4] != tag {
            return None;
        }
        let off = u32::from_be_bytes(r[8..12].try_into().ok()?) as usize;
        let len = u32::from_be_bytes(r[12..16].try_into().ok()?) as usize;
        font.get(off..off + len)
    })
}

/// Write one font's objects from `first`.
fn write_font(o: &mut Objects, first: usize, fu: &FontUse) {
    let f = fu.font;
    let [type0, cid_font, descriptor, program_id, to_unicode, cid_to_gid] = std::array::from_fn(|i| first + i);
    // The subset: the remapper of CFF fonts, else every glyph the codes use, in order.
    let remap = fu.remap.clone().unwrap_or_else(|| {
        let mut gids: Vec<u16> = fu.codes.iter().flatten().map(|c| c.0).collect();
        gids.sort_unstable();
        gids.dedup();
        subsetter::GlyphRemapper::new_from_glyphs_sorted(&gids)
    });
    // Fonts were checked to subset when they were loaded.
    let sub = subsetter::subset(f.data, f.index, &remap).unwrap_or_default();
    let program: Vec<u8> = if f.cff { table(&sub, b"CFF ").map(<[u8]>::to_vec).unwrap_or_default() } else { sub };
    let [tag_bits, _] = digest(&program);
    let tag: String = (0..6).map(|i| char::from(b'A' + ((tag_bits >> (5 * i)) % 26) as u8)).collect();
    let base_font = format!("/{tag}+{}", f.name);
    o.object(
        type0,
        format!("<< /Type /Font /Subtype /Type0 /BaseFont {base_font} /Encoding /Identity-H /DescendantFonts [{cid_font} 0 R] /ToUnicode {to_unicode} 0 R >>").as_bytes(),
    );
    let widths: Vec<String> = fu.codes.iter().enumerate().skip(1).filter_map(|(c, x)| x.as_ref().map(|(g, _)| format!("{c} [{}]", f.advance_1000(*g)))).collect();
    let (subtype, gid_map) = if f.cff { ("/CIDFontType0", String::new()) } else { ("/CIDFontType2", format!(" /CIDToGIDMap {cid_to_gid} 0 R")) };
    o.object(
        cid_font,
        format!(
            "<< /Type /Font /Subtype {subtype} /BaseFont {base_font} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /DW 1000 /W [{}]{gid_map} >>",
            widths.join(" ")
        )
        .as_bytes(),
    );
    let s = |v: i16| i32::from(v) * 1000 / i32::from(f.upem.max(1));
    let [x0, y0, x1, y1] = f.bbox;
    let file_key = if f.cff { "/FontFile3" } else { "/FontFile2" };
    o.object(
        descriptor,
        format!(
            "<< /Type /FontDescriptor /FontName {base_font} /Flags 4 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV 80 {file_key} {program_id} 0 R >>",
            s(x0),
            s(y0),
            s(x1),
            s(y1),
            s(f.ascent),
            s(f.descent),
            s(f.cap_height)
        )
        .as_bytes(),
    );
    let packed = deflate(&program);
    if f.cff {
        o.stream(program_id, "/Subtype /CIDFontType0C /Filter /FlateDecode", &packed);
    } else {
        o.stream(program_id, &format!("/Filter /FlateDecode /Length1 {}", program.len()), &packed);
    }
    o.stream(to_unicode, "/Filter /FlateDecode", &deflate(to_unicode_cmap(&fu.codes).as_bytes()));
    // Code → subset glyph, two bytes each (CFF fonts need none; an empty map keeps the numbering).
    let mut map = Vec::with_capacity(2 * fu.codes.len());
    if !f.cff {
        for c in &fu.codes {
            map.extend_from_slice(&c.as_ref().and_then(|(g, _)| remap.get(*g)).unwrap_or(0).to_be_bytes());
        }
    }
    o.stream(cid_to_gid, "/Filter /FlateDecode", &deflate(&map));
}

/// The PDF file.
pub(crate) fn write(d: &Display) -> Vec<u8> {
    let mut fonts = Fonts::new(d);
    let mut o = Objects { out: b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec(), offsets: vec![0] };

    // Object numbers: fonts, images, then pages.
    let mut next = FIRST_FREE;
    let font_ids: Vec<usize> = fonts
        .uses
        .iter()
        .map(|_| {
            next += FONT_OBJECTS;
            next - FONT_OBJECTS
        })
        .collect();
    let mut image_ids = Vec::new();
    for img in &d.images {
        let id = next;
        next += 1;
        let mask = img.alpha.as_ref().map(|_| {
            next += 1;
            next - 1
        });
        image_ids.push((id, mask));
    }
    let page_ids: Vec<(usize, usize)> = d
        .pages
        .iter()
        .map(|_| {
            next += 2;
            (next - 2, next - 1)
        })
        .collect();

    o.object(CATALOG, format!("<< /Type /Catalog /Pages {PAGES} 0 R >>").as_bytes());
    let kids: Vec<String> = page_ids.iter().map(|(p, _)| format!("{p} 0 R")).collect();
    o.object(PAGES, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), page_ids.len()).as_bytes());
    o.object(INFO, format!("<< /Producer (KhataOS) /Creator (KhataOS) /Title {} >>", text_string(&d.title)).as_bytes());

    for (img, &(id, mask)) in d.images.iter().zip(&image_ids) {
        let mut dict = format!("/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8 /Filter {}", img.width, img.height, img.colour_space, img.filter);
        if img.invert_cmyk {
            dict.push_str(" /Decode [1 0 1 0 1 0 1 0]");
        }
        if let Some(m) = mask {
            let _ = write!(dict, " /SMask {m} 0 R");
        }
        let data = if img.filter == "/FlateDecode" { deflate(&img.data) } else { img.data.clone() };
        o.stream(id, &dict, &data);
        if let (Some(m), Some(a)) = (mask, &img.alpha) {
            o.stream(
                m,
                &format!("/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode", img.width, img.height),
                &deflate(a),
            );
        }
    }

    // Pages first: their content gives out the codes the fonts then embed.
    for (page, &(pid, cid)) in d.pages.iter().zip(&page_ids) {
        let used_images: BTreeSet<usize> = page.ops.iter().filter_map(|op| if let Op::Image { id, .. } = op { Some(*id) } else { None }).collect();
        let xobjects: Vec<String> = used_images.iter().filter_map(|&i| image_ids.get(i).map(|(obj, _)| format!("/Im{i} {obj} 0 R"))).collect();
        let xobject = if xobjects.is_empty() { String::new() } else { format!(" /XObject << {} >>", xobjects.join(" ")) };
        let used_fonts: BTreeSet<usize> = page.ops.iter().flat_map(|op| if let Op::Text { glyphs, .. } = op { glyphs.as_slice() } else { &[] }).map(|g| fonts.by_id[&g.font]).collect();
        let font_dict: Vec<String> = used_fonts.iter().map(|&i| format!("/F{} {} 0 R", fonts.uses[i].resource, font_ids[i])).collect();
        let stream = content(page, &mut fonts);
        o.object(
            pid,
            format!(
                "<< /Type /Page /Parent {PAGES} 0 R /MediaBox [0 0 {} {}] /Resources << /Font << {} >>{xobject} >> /Contents {cid} 0 R >>",
                n(page.w),
                n(page.h),
                font_dict.join(" ")
            )
            .as_bytes(),
        );
        o.stream(cid, "/Filter /FlateDecode", &deflate(&stream));
    }
    for (fu, &first) in fonts.uses.iter().zip(&font_ids) {
        write_font(&mut o, first, fu);
    }

    let [a, b] = digest(&o.out);
    let id = format!("<{a:016X}{b:016X}>");
    let xref = o.out.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", o.offsets.len());
    for off in &o.offsets[1..] {
        let _ = writeln!(tail, "{off:010} 00000 n ");
    }
    let _ = write!(tail, "trailer\n<< /Size {} /Root {CATALOG} 0 R /Info {INFO} 0 R /ID [{id} {id}] >>\nstartxref\n{xref}\n%%EOF\n", o.offsets.len());
    o.out.extend_from_slice(tail.as_bytes());
    o.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short() {
        assert_eq!(n(12.0), "12");
        assert_eq!(n(0.1234), "0.123");
        assert_eq!(n(-0.0001), "0");
        assert_eq!(n(-2.5), "-2.5");
    }

    #[test]
    fn titles_are_utf16() {
        assert_eq!(text_string("A€"), "<FEFF004120AC>");
    }
}

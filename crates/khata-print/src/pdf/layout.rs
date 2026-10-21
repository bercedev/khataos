//! Laying out invoices on pages: the display list for the writer, and the
//! text of every run drawn (for tests and for what the PDF is known to show).
//!
//! Every document prints the same content in the same order whatever the
//! template; the template only moves the header around and changes sizes,
//! rules and colours. See `pdf/mod.rs` for the list.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use khata_core::Amount;
use khata_core::format::format_amount;

use super::image::{self, PdfImage};
use super::shape::{self, Dir, Line};
use super::writer::{Display, Op, Page, Rgb};
use super::{PdfDoc, PdfLabels};
use crate::{ArabicLabels, Header, Image, InvoiceDoc, Look, QrSymbol, Template};

const MM: f32 = 72.0 / 25.4;
const BLACK: Rgb = (0.07, 0.07, 0.07);
const GREY: Rgb = (0.33, 0.33, 0.33);
const RULE: Rgb = (0.72, 0.72, 0.72);
const WHITE: Rgb = (1.0, 1.0, 1.0);
const VOID: Rgb = (0.65, 0.0, 0.0);
/// The page height a receipt is laid out on before it is cut to its content.
const ROLL: f32 = 20_000.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Centre,
    Right,
}

/// Sizes and colours of one document's template.
#[derive(Clone, Copy)]
struct Style {
    base: f32,
    small: f32,
    title: f32,
    company: f32,
    /// Cell padding.
    pad: f32,
    /// Space between sections.
    gap: f32,
    accent: Rgb,
    template: Template,
}

/// A text line in a box: (text, size, bold, colour).
type BoxLine = (String, f32, bool, Rgb);

/// The language of a document's labels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lang {
    English,
    /// "English / Arabic", left to right.
    Bilingual,
    /// Arabic, right to left.
    Arabic,
}

/// An Arabic run inside a left-to-right line, as the HTML's `dir="rtl"` span.
const RLI: char = '\u{2067}';
/// Text from the books inside an Arabic line, as the HTML's `<bdi>`.
const FSI: char = '\u{2068}';
const PDI: char = '\u{2069}';

pub(crate) struct Layout {
    pub display: Display,
    pub texts: Vec<String>,
    /// QR codes drawn (tests: the tax authorities' QR codes are on the page).
    pub qr_codes: usize,
    /// Images already added, by (data address, length): index or failure.
    image_cache: HashMap<(usize, usize), Option<usize>>,
    /// Lines already shaped (wrapping measures the same words again and again).
    shaped: RefCell<HashMap<(String, Dir), Rc<Line>>>,
    // The page being filled.
    w: f32,
    h: f32,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    y: f32,
    thermal: bool,
    lang: Lang,
    /// The page is right to left (Arabic): everything is mirrored.
    rtl: bool,
    /// Amounts, dates and page numbers in Arabic-Indic digits.
    arabic_digits: bool,
    st: Style,
}

fn lh(size: f32) -> f32 {
    size * 1.3
}

/// A light tint of a colour (towards white).
fn tint(c: Rgb, k: f32) -> Rgb {
    (c.0 + (1.0 - c.0) * k, c.1 + (1.0 - c.1) * k, c.2 + (1.0 - c.2) * k)
}

impl Layout {
    pub fn new() -> Self {
        Layout {
            display: Display::default(),
            texts: Vec::new(),
            qr_codes: 0,
            image_cache: HashMap::new(),
            shaped: RefCell::new(HashMap::new()),
            w: 0.0,
            h: 0.0,
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
            y: 0.0,
            thermal: false,
            lang: Lang::English,
            rtl: false,
            arabic_digits: false,
            st: Style { base: 9.5, small: 8.0, title: 12.0, company: 14.0, pad: 3.0, gap: 8.0, accent: BLACK, template: Template::Classic },
        }
    }

    // ------------------------------------------------------------ text

    /// A line shaped (bidi, fonts, OpenType), from the cache.
    fn shaped(&self, s: &str) -> Rc<Line> {
        let dir = if self.rtl { Dir::Rtl } else { Dir::Ltr };
        let key = (s.to_owned(), dir);
        if let Some(l) = self.shaped.borrow().get(&key) {
            return l.clone();
        }
        let l = Rc::new(shape::shape(s, dir));
        self.shaped.borrow_mut().insert(key, l.clone());
        l
    }

    fn width(&self, s: &str, size: f32) -> f32 {
        self.shaped(s).width * size / 1000.0
    }

    /// Right-to-left pages are laid out left to right and mirrored here.
    fn mirror(&self, op: Op) -> Op {
        if !self.rtl {
            return op;
        }
        let pw = self.w;
        match op {
            Op::Text { x, y, w, size, bold, colour, glyphs } => Op::Text { x: pw - x - w, y, w, size, bold, colour, glyphs },
            Op::Rect { x, y, w, h, fill, stroke } => Op::Rect { x: pw - x - w, y, w, h, fill, stroke },
            Op::Line { x1, y1, x2, y2, colour, width } => Op::Line { x1: pw - x1, y1, x2: pw - x2, y2, colour, width },
            Op::Image { id, x, y, w, h } => Op::Image { id, x: pw - x - w, y, w, h },
        }
    }

    fn op(&mut self, op: Op) {
        let op = self.mirror(op);
        self.op_unmirrored(op);
    }

    fn op_unmirrored(&mut self, op: Op) {
        if let Some(p) = self.display.pages.last_mut() {
            p.ops.push(op);
        }
    }

    /// One line as a drawing operation (not mirrored yet). `x` is the left
    /// edge, the centre or the right edge. Records the text.
    #[allow(clippy::too_many_arguments)]
    fn run(&mut self, x: f32, baseline: f32, s: &str, size: f32, bold: bool, colour: Rgb, align: Align) -> Option<Op> {
        if s.trim().is_empty() {
            return None;
        }
        let line = self.shaped(s);
        let w = line.width * size / 1000.0;
        let x = match align {
            Align::Left => x,
            Align::Centre => x - w / 2.0,
            Align::Right => x - w,
        };
        self.texts.push(shape::plain(s));
        Some(Op::Text { x, y: baseline, w, size, bold, colour, glyphs: line.glyphs.clone() })
    }

    /// Draw one line on the current page.
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, x: f32, baseline: f32, s: &str, size: f32, bold: bool, colour: Rgb, align: Align) {
        if let Some(op) = self.run(x, baseline, s, size, bold, colour, align) {
            self.op(op);
        }
    }

    /// `s` broken into lines no wider than `width` (at spaces; a word that
    /// doesn't fit is broken anywhere). New lines in `s` are kept.
    fn wrap(&self, s: &str, size: f32, width: f32) -> Vec<String> {
        let mut out = Vec::new();
        for para in s.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                let candidate = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
                if self.width(&candidate, size) <= width || candidate.is_empty() {
                    line = candidate;
                    continue;
                }
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                // The word alone; break it (between clusters) if it is still too wide.
                let mut from = 0;
                let mut last = 0;
                for b in shape::breaks(word).into_iter().chain(std::iter::once(word.len())) {
                    if self.width(&word[from..b], size) > width && last > from {
                        out.push(word[from..last].to_owned());
                        from = last;
                    }
                    last = b;
                }
                line = word[from..].to_owned();
            }
            out.push(line);
        }
        out
    }

    /// A wrapped paragraph from the current position, page breaks between lines.
    #[allow(clippy::too_many_arguments)]
    fn para(&mut self, x: f32, width: f32, s: &str, size: f32, bold: bool, colour: Rgb, align: Align) {
        // An Arabic line in a left-to-right page starts at the right (a
        // right-to-left page already starts every line there).
        let align = if align == Align::Left && !self.rtl && shape::starts_rtl(s) && !self.thermal { Align::Right } else { align };
        for line in self.wrap(s, size, width) {
            self.ensure(lh(size));
            let at = match align {
                Align::Left => x,
                Align::Centre => x + width / 2.0,
                Align::Right => x + width,
            };
            self.text(at, self.y + size, &line, size, bold, colour, align);
            self.y += lh(size);
        }
    }

    // ------------------------------------------------------------ pages

    fn content_width(&self) -> f32 {
        self.w - self.left - self.right
    }

    fn new_page(&mut self) {
        self.display.pages.push(Page { w: self.w, h: self.h, ops: Vec::new() });
        self.y = self.top;
    }

    /// Start a new page when `height` doesn't fit; true when it did.
    fn ensure(&mut self, height: f32) -> bool {
        if !self.thermal && self.y + height > self.h - self.bottom && self.y > self.top + 1.0 {
            self.new_page();
            return true;
        }
        false
    }

    fn rule(&mut self, colour: Rgb, width: f32) {
        let (l, r) = (self.left, self.w - self.right);
        let y = self.y;
        self.op(Op::Line { x1: l, y1: y, x2: r, y2: y, colour, width });
    }

    // ------------------------------------------------------------ labels

    /// A label: English, "English / Arabic" (the Arabic isolated, so a
    /// colon and value after it stay after it) or Arabic.
    fn bi(&self, en: &str, ar: &str) -> String {
        match self.lang {
            Lang::Bilingual if !ar.is_empty() => format!("{en} / {RLI}{ar}{PDI}"),
            Lang::Arabic if !ar.is_empty() => ar.to_owned(),
            _ => en.to_owned(),
        }
    }

    /// Text from the books after an Arabic label, in its own direction.
    fn val(&self, v: &str) -> String {
        if self.lang == Lang::Arabic { format!("{FSI}{v}{PDI}") } else { v.to_owned() }
    }

    /// Digits as the document prints them (Arabic-Indic when asked).
    fn num(&self, s: &str) -> String {
        if !self.arabic_digits {
            return s.to_owned();
        }
        let cs: Vec<char> = s.chars().collect();
        let digit = |i: usize| cs.get(i).is_some_and(char::is_ascii_digit);
        cs.iter()
            .enumerate()
            .map(|(i, &c)| match c {
                '0'..='9' => char::from_u32(0x0660 + (c as u32 - '0' as u32)).unwrap_or(c),
                '.' if i > 0 && digit(i - 1) && digit(i + 1) => '\u{066B}',
                ',' if i > 0 && digit(i - 1) && digit(i + 1) => '\u{066C}',
                c => c,
            })
            .collect()
    }

    fn date(&self, d: chrono::NaiveDate) -> String {
        if self.lang == Lang::Arabic { self.num(&crate::date_ar(d)) } else { crate::date(d) }
    }

    /// A label added with invoice designs (`labels.rs`).
    fn nl(&self, en: &str) -> String {
        self.bi(en, crate::labels::arabic(en).unwrap_or(""))
    }

    fn amount(&self, a: Amount, h: &Header) -> String {
        self.num(&format_amount(a, h.grouping, 2))
    }

    // ------------------------------------------------------------ images

    fn image_id(&mut self, img: &Image) -> Option<usize> {
        let (kind, bytes) = img.bitmap()?;
        let key = (bytes.as_ptr() as usize, bytes.len());
        if let Some(&id) = self.image_cache.get(&key) {
            return id;
        }
        let id = image::load(kind, bytes).map(|i: PdfImage| {
            self.display.images.push(i);
            self.display.images.len() - 1
        });
        self.image_cache.insert(key, id);
        id
    }

    /// Draw an image to fit `max_w` × `max_h` at the current y; returns its size.
    fn image(&mut self, img: &Image, x: f32, max_w: f32, max_h: f32, align: Align) -> Option<(f32, f32)> {
        let id = self.image_id(img)?;
        let (iw, ih) = {
            let i = &self.display.images[id];
            (i.width as f32, i.height as f32)
        };
        let k = (max_w / iw).min(max_h / ih);
        let (w, h) = (iw * k, ih * k);
        let x = match align {
            Align::Left => x,
            Align::Centre => x - w / 2.0,
            Align::Right => x - w,
        };
        let y = self.y;
        self.op(Op::Image { id, x, y, w, h });
        Some((w, h))
    }

    /// A QR code with its quiet zone, `size` points square, top left at (x, y).
    fn qr(&mut self, q: &QrSymbol, x: f32, y: f32, size: f32) {
        self.qr_codes += 1;
        let side = (q.width + 8) as f32;
        let m = size / side;
        // On a right-to-left page the code moves, but is never mirrored itself.
        let x = if self.rtl { self.w - x - size } else { x };
        self.op_unmirrored(Op::Rect { x, y, w: size, h: size, fill: Some(WHITE), stroke: None });
        for row in 0..q.width {
            let mut col = 0;
            while col < q.width {
                if q.dark.get(row * q.width + col).copied().unwrap_or(false) {
                    let start = col;
                    while col < q.width && q.dark.get(row * q.width + col).copied().unwrap_or(false) {
                        col += 1;
                    }
                    let rx = x + (start + 4) as f32 * m;
                    let ry = y + (row + 4) as f32 * m;
                    // A hair wider, so neighbouring runs don't show seams.
                    self.op_unmirrored(Op::Rect { x: rx, y: ry, w: (col - start) as f32 * m + 0.05, h: m + 0.05, fill: Some(BLACK), stroke: None });
                } else {
                    col += 1;
                }
            }
        }
    }

    // ------------------------------------------------------------ the document

    pub fn document(&mut self, h: &Header, pd: &PdfDoc) {
        let look = &pd.look;
        let d = pd.doc;
        (self.lang, self.arabic_digits) = match pd.labels {
            PdfLabels::English => (Lang::English, false),
            PdfLabels::Bilingual => (Lang::Bilingual, false),
            PdfLabels::Arabic { arabic_indic_digits } => (Lang::Arabic, arabic_indic_digits),
        };
        self.rtl = self.lang == Lang::Arabic;
        self.thermal = look.thermal();
        let (wmm, hmm) = look.paper.size_mm();
        let a5 = look.paper == crate::Paper::A5;
        self.w = wmm * MM;
        self.h = if self.thermal { ROLL } else { hmm * MM };
        let (tb, side) = if self.thermal {
            (3.0, 3.0)
        } else if a5 {
            (10.0, 9.0)
        } else {
            (14.0, 12.0)
        };
        self.top = tb * MM;
        self.bottom = tb * MM;
        self.left = side * MM;
        self.right = side * MM;
        let template = look.design.template;
        let mut base = match template {
            Template::Compact => 8.0,
            _ => 9.5,
        };
        if a5 {
            base *= 0.92;
        }
        if self.thermal {
            base = 7.5;
        }
        self.st = Style {
            base,
            small: base * 0.85,
            title: if template == Template::Modern && !self.thermal { base * 1.9 } else { base * 1.35 },
            company: base * 1.5,
            pad: if template == Template::Compact || self.thermal { 2.0 } else { 3.0 },
            gap: if template == Template::Compact || self.thermal { 5.0 } else { 8.0 },
            accent: look.design.accent_rgb(),
            template,
        };
        let first_page = self.display.pages.len();
        self.new_page();

        self.header(h, d, look);
        self.cancelled(d);
        self.blocks(d);
        self.parties(d);
        self.currency(h, d);
        self.export(d);
        self.lines(h, d, look);
        self.closing(h, d, look);

        if self.thermal {
            self.y += self.st.gap;
            let y = self.y + self.st.small;
            let centre = self.w / 2.0;
            let label = self.footer_label(1, 1);
            self.text(centre, y, &label, self.st.small * 0.85, false, GREY, Align::Centre);
            self.y += lh(self.st.small);
            let height = self.y + self.bottom;
            if let Some(p) = self.display.pages.last_mut() {
                p.h = height;
            }
        } else {
            let pages = self.display.pages.len() - first_page;
            for i in 0..pages {
                let baseline = self.h - self.bottom * 0.45;
                let label = self.footer_label(i + 1, pages);
                if let Some(op) = self.run(self.w / 2.0, baseline, &label, self.st.small * 0.85, false, GREY, Align::Centre) {
                    let op = self.mirror(op);
                    self.display.pages[first_page + i].ops.push(op);
                }
            }
        }
    }

    fn footer_label(&self, page: usize, pages: usize) -> String {
        if self.lang == Lang::Arabic {
            format!("أنشئ بواسطة {FSI}KhataOS{PDI} · صفحة {} من {}", self.num(&page.to_string()), self.num(&pages.to_string()))
        } else {
            format!("Generated by KhataOS · Page {page} of {pages}")
        }
    }

    fn title(&self, d: &InvoiceDoc) -> String {
        let ar = ArabicLabels::default();
        let arabic = ar.title(&d.title).or_else(|| crate::voucher_title_ar(&d.title)).unwrap_or("");
        self.bi(&d.title, arabic)
    }

    /// A tax registration label ("VAT number", "TRN", "GSTIN").
    fn tax_label(&self, label: &str) -> String {
        let ar = ArabicLabels::default();
        match self.lang {
            Lang::Arabic => match label.trim().to_ascii_lowercase().as_str() {
                "vat number" | "vat no." | "vat reg no" | "vat registration number" => ar.vat_number,
                "trn" => format!("رقم التسجيل الضريبي ({FSI}TRN{PDI})"),
                _ => label.to_owned(),
            },
            _ => self.bi(label, &ar.vat_number),
        }
    }

    fn seller_tax(&self, d: &InvoiceDoc) -> Option<String> {
        let (label, id) = d.seller_tax.as_ref()?;
        let region = d.seller_region.as_deref().map(|r| format!(" · {r}")).unwrap_or_default();
        Some(format!("{}: {}{region}", self.tax_label(label), self.val(id)))
    }

    fn header(&mut self, h: &Header, d: &InvoiceDoc, look: &Look) {
        let st = self.st;
        let cw = self.content_width();
        let (l, r) = (self.left, self.w - self.right);
        let top = self.y;
        let title = self.title(d);
        let tax = self.seller_tax(d);
        let qs = if self.thermal { 32.0 * MM } else { 30.0 * MM };
        if self.thermal {
            let c = self.w / 2.0;
            if let Some(logo) = look.logo
                && let Some((_, ih)) = self.image(logo, c, 50.0 * MM, 18.0 * MM, Align::Centre)
            {
                self.y += ih + 3.0;
            }
            self.para(l, cw, h.company, st.company, true, BLACK, Align::Centre);
            self.para(l, cw, h.address, st.small, false, GREY, Align::Centre);
            if let Some(t) = &tax {
                self.para(l, cw, t, st.small, false, BLACK, Align::Centre);
            }
            self.y += 2.0;
            self.para(l, cw, &title, st.title, true, st.accent, Align::Centre);
            if let Some(q) = &d.seller_qr {
                self.y += 2.0;
                let y = self.y;
                self.qr(q, c - qs / 2.0, y, qs);
                self.y += qs + 2.0;
            }
            self.y += 2.0;
            self.op(Op::Line { x1: l, y1: self.y, x2: r, y2: self.y, colour: RULE, width: 0.5 });
            self.y += st.gap;
            return;
        }
        match st.template {
            Template::Classic => {
                if let Some(q) = &d.seller_qr {
                    self.qr(q, r - qs, top, qs);
                }
                let reserve = if d.seller_qr.is_some() { qs + 6.0 } else { 0.0 };
                let (cl, ccw) = (l + reserve, cw - 2.0 * reserve);
                if let Some(logo) = look.logo
                    && let Some((_, ih)) = self.image(logo, l + cw / 2.0, 60.0 * MM, 18.0 * MM, Align::Centre)
                {
                    self.y += ih + 4.0;
                }
                self.para(cl, ccw, h.company, st.company, true, BLACK, Align::Centre);
                self.para(cl, ccw, h.address, st.small, false, GREY, Align::Centre);
                if let Some(t) = &tax {
                    self.para(cl, ccw, t, st.small, false, BLACK, Align::Centre);
                }
                self.y += 3.0;
                self.para(cl, ccw, &title, st.title, true, st.accent, Align::Centre);
                if d.seller_qr.is_some() {
                    self.y = self.y.max(top + qs);
                }
                self.y += 4.0;
                self.rule(st.accent, 0.75);
            }
            Template::Modern | Template::Compact => {
                let modern = st.template == Template::Modern;
                let rw = if modern { cw * 0.42 } else { cw * 0.45 };
                let lw = cw - rw - 8.0;
                let mut tx = l;
                let mut left_bottom = top;
                let (logo_w, logo_h) = if modern { (45.0 * MM, 20.0 * MM) } else { (35.0 * MM, 14.0 * MM) };
                if let Some(logo) = look.logo
                    && let Some((iw, ih)) = self.image(logo, l, logo_w.min(lw * 0.45), logo_h, Align::Left)
                {
                    tx = l + iw + 6.0;
                    left_bottom = top + ih;
                }
                let tw = lw - (tx - l);
                self.para(tx, tw, h.company, st.company, true, if modern { st.accent } else { BLACK }, Align::Left);
                self.para(tx, tw, h.address, st.small, false, GREY, Align::Left);
                if let Some(t) = &tax {
                    self.para(tx, tw, t, st.small, false, BLACK, Align::Left);
                }
                left_bottom = left_bottom.max(self.y);
                // The right column: the title, then the QR.
                self.y = top;
                self.para(r - rw, rw, &title, st.title, true, st.accent, Align::Right);
                if let Some(q) = &d.seller_qr {
                    self.y += 3.0;
                    let y = self.y;
                    self.qr(q, r - qs, y, qs);
                    self.y += qs;
                }
                self.y = self.y.max(left_bottom) + 4.0;
                self.rule(st.accent, if modern { 1.5 } else { 0.75 });
            }
        }
        self.y += st.gap;
    }

    fn cancelled(&mut self, d: &InvoiceDoc) {
        if !d.cancelled {
            return;
        }
        let st = self.st;
        let size = st.title;
        let label = self.bi("CANCELLED", &ArabicLabels::default().cancelled);
        let (l, cw) = (self.left, self.content_width());
        let y = self.y;
        self.op(Op::Rect { x: l, y, w: cw, h: lh(size) + 4.0, fill: None, stroke: Some((VOID, 1.0)) });
        self.text(l + cw / 2.0, y + 2.0 + size, &label, size, true, VOID, Align::Centre);
        self.y += lh(size) + 4.0 + st.gap;
    }

    fn blocks(&mut self, d: &InvoiceDoc) {
        let st = self.st;
        let size = st.small;
        for b in &d.blocks {
            let qs = if b.qr.is_some() { (if self.thermal { 30.0 } else { 28.0 }) * MM } else { 0.0 };
            let side_qr = b.qr.is_some() && !self.thermal;
            let tw = self.content_width() - if side_qr { qs + 8.0 } else { 0.0 };
            let mut lines: Vec<(String, bool)> = Vec::new();
            for f in &b.flags {
                lines.extend(self.wrap(f, size, tw).into_iter().map(|s| (s, true)));
            }
            for (label, value) in &b.rows {
                lines.extend(self.wrap(&format!("{label}: {value}"), size, tw).into_iter().map(|s| (s, false)));
            }
            if !b.note.trim().is_empty() {
                lines.extend(self.wrap(&b.note, size, tw).into_iter().map(|s| (s, false)));
            }
            let text_h = lines.len() as f32 * lh(size);
            let height = if side_qr { text_h.max(qs) } else { text_h + qs + 4.0 };
            self.ensure(height.min(self.h - self.top - self.bottom));
            let top = self.y;
            if let (true, Some(q)) = (side_qr, &b.qr) {
                let x = self.w - self.right - qs;
                self.qr(q, x, top, qs);
            }
            let l = self.left;
            for (line, bold) in lines {
                self.ensure(lh(size));
                self.text(l, self.y + size, &line, size, bold, BLACK, Align::Left);
                self.y += lh(size);
            }
            if let (false, Some(q)) = (side_qr, &b.qr) {
                self.y += 2.0;
                let (x, y) = (self.w / 2.0 - qs / 2.0, self.y);
                self.qr(q, x, y, qs);
                self.y += qs;
            }
            if side_qr {
                self.y = self.y.max(top + qs);
            }
            self.y += st.gap;
        }
    }

    /// Lines of a box: a small grey label, then values.
    fn label_line(&self, s: String) -> BoxLine {
        (s, self.st.small, false, GREY)
    }

    fn parties(&mut self, d: &InvoiceDoc) {
        let st = self.st;
        let ar = ArabicLabels::default();
        let mut boxes: Vec<Vec<BoxLine>> = Vec::new();
        let english = self.lang == Lang::English;
        let mut buyer = vec![self.label_line(if english { "Bill to".into() } else { self.bi("Buyer", &ar.buyer) }), (d.buyer_name.clone(), st.base, true, BLACK)];
        buyer.extend(d.buyer_address.lines().filter(|l| !l.trim().is_empty()).map(|l| (l.to_owned(), st.small, false, BLACK)));
        if let Some((label, id)) = &d.buyer_tax {
            buyer.push((format!("{}: {}", self.tax_label(label), self.val(id)), st.small, false, BLACK));
        }
        boxes.push(buyer);
        if let Some(s) = &d.ship_to {
            let mut ship = vec![self.label_line(self.nl("Ship to")), (s.name.clone(), st.base, true, BLACK)];
            ship.extend(s.address.lines().filter(|l| !l.trim().is_empty()).map(|l| (l.to_owned(), st.small, false, BLACK)));
            boxes.push(ship);
        }
        let mut meta = vec![
            self.label_line(self.bi(if english { "Number" } else { "Invoice number" }, &ar.invoice_number)),
            (d.number.clone(), st.base, true, BLACK),
            self.label_line(self.bi("Date", &ar.date)),
            (self.date(d.date), st.base, false, BLACK),
        ];
        if let Some(hj) = d.hijri.as_deref().filter(|s| !s.trim().is_empty()) {
            meta.push((hj.to_owned(), st.small, false, GREY));
        }
        if let Some(p) = &d.place_of_supply {
            meta.push(self.label_line(self.bi("Place of supply", &ar.place_of_supply)));
            meta.push((p.clone(), st.base, false, BLACK));
        }
        boxes.push(meta);

        let cw = self.content_width();
        if self.thermal {
            for b in boxes {
                for (s, size, bold, c) in b {
                    let l = self.left;
                    self.para(l, cw, &s, size, bold, c, Align::Left);
                }
                self.y += 2.0;
                let (l, r, y) = (self.left, self.w - self.right, self.y);
                self.op(Op::Line { x1: l, y1: y, x2: r, y2: y, colour: RULE, width: 0.4 });
                self.y += 3.0;
            }
            return;
        }
        let gap = 8.0;
        let n = boxes.len() as f32;
        let bw = (cw - gap * (n - 1.0)) / n;
        let inner = bw - 2.0 * st.pad - 2.0;
        let wrapped: Vec<Vec<BoxLine>> = boxes
            .iter()
            .map(|b| b.iter().flat_map(|(s, size, bold, c)| self.wrap(s, *size, inner).into_iter().map(move |w| (w, *size, *bold, *c))).collect())
            .collect();
        let height = wrapped.iter().map(|b| b.iter().map(|l| lh(l.1)).sum::<f32>()).fold(0.0, f32::max) + 2.0 * st.pad + 2.0;
        self.ensure(height);
        let top = self.y;
        for (i, b) in wrapped.into_iter().enumerate() {
            let x = self.left + i as f32 * (bw + gap);
            let (fill, stroke) = match st.template {
                Template::Modern => (Some(tint(st.accent, 0.92)), None),
                _ => (None, Some((RULE, 0.5))),
            };
            self.op(Op::Rect { x, y: top, w: bw, h: height, fill, stroke });
            let mut y = top + st.pad + 1.0;
            for (s, size, bold, c) in b {
                let rtl = !self.rtl && shape::starts_rtl(&s);
                let (at, align) = if rtl { (x + bw - st.pad - 1.0, Align::Right) } else { (x + st.pad + 1.0, Align::Left) };
                self.text(at, y + size, &s, size, bold, c, align);
                y += lh(size);
            }
        }
        self.y = top + height + st.gap;
    }

    /// "Label: value" rows under a heading, in two columns on wide paper.
    fn kv(&mut self, heading: &str, rows: &[(String, String)]) {
        if rows.is_empty() {
            return;
        }
        let st = self.st;
        let size = st.small;
        let cw = self.content_width();
        let two = !self.thermal && rows.len() >= 3;
        let colw = if two { (cw - 10.0) / 2.0 } else { cw };
        let split = if two { rows.len().div_ceil(2) } else { rows.len() };
        let cols: Vec<Vec<String>> = [&rows[..split], &rows[split..]]
            .iter()
            .map(|part| part.iter().flat_map(|(l, v)| self.wrap(&format!("{l}: {}", self.val(v)), size, colw)).collect())
            .collect();
        let head_h = if heading.is_empty() { 0.0 } else { lh(st.base) };
        let height = head_h + cols.iter().map(Vec::len).max().unwrap_or(0) as f32 * lh(size);
        self.ensure(height.min(self.h - self.top - self.bottom));
        if !heading.is_empty() {
            let l = self.left;
            self.text(l, self.y + st.base, heading, st.base, true, st.accent, Align::Left);
            self.y += head_h;
        }
        let top = self.y;
        let mut bottom = top;
        for (i, col) in cols.into_iter().enumerate() {
            self.y = top;
            let x = self.left + i as f32 * (colw + 10.0);
            for line in col {
                let page_before = self.display.pages.len();
                self.ensure(lh(size));
                if self.display.pages.len() != page_before {
                    bottom = self.top;
                }
                self.text(x, self.y + size, &line, size, false, BLACK, Align::Left);
                self.y += lh(size);
            }
            bottom = bottom.max(self.y);
        }
        self.y = bottom + st.gap;
    }

    fn currency(&mut self, h: &Header, d: &InvoiceDoc) {
        let Some(c) = &d.currency else { return };
        let (en, ar) = c.headline();
        let heading = self.bi(&en, &ar);
        let rows: Vec<(String, String)> = c.base_figures().into_iter().map(|(en, ar, a)| (self.bi(&en, &ar), self.amount(a, h))).collect();
        self.kv(&heading, &rows);
    }

    fn export(&mut self, d: &InvoiceDoc) {
        let Some(e) = d.export.as_ref().filter(|e| !e.is_empty()) else { return };
        let mut rows = Vec::new();
        for (label, v) in [
            ("Port of loading", &e.port_of_loading),
            ("Port of discharge", &e.port_of_discharge),
            ("Incoterms", &e.incoterms),
            ("Country of destination", &e.destination_country),
            ("Shipping bill no.", &e.shipping_bill_no),
        ] {
            if !v.trim().is_empty() {
                rows.push((self.nl(label), v.clone()));
            }
        }
        if let Some(sd) = e.shipping_bill_date {
            rows.push((self.nl("Shipping bill date"), self.date(sd)));
        }
        let heading = self.nl("Export details");
        self.kv(&heading, &rows);
    }

    fn lines(&mut self, h: &Header, d: &InvoiceDoc, look: &Look) {
        let st = self.st;
        let ar = ArabicLabels::default();
        let has_class = look.show_tax_class() && d.lines.iter().any(|l| l.tax_class.is_some());
        let has_qty = look.show_qty_rate() && d.lines.iter().any(|l| l.qty.is_some());
        let taxable: Amount = d.lines.iter().map(|l| l.amount).sum();
        let mut totals: Vec<(String, Amount, bool)> = Vec::new();
        let english = self.lang == Lang::English;
        if !d.taxes.is_empty() || !d.others.is_empty() {
            totals.push((if english { "Taxable value".into() } else { self.bi("Total excluding VAT", &ar.taxable_amount) }, taxable, false));
        }
        for (label, a) in &d.taxes {
            let l = if label.to_ascii_lowercase().contains("vat") { self.bi(label, &ar.vat) } else { label.clone() };
            totals.push((l, *a, false));
        }
        for (label, a) in &d.others {
            totals.push((label.clone(), *a, false));
        }
        totals.push((if english { "Total".into() } else { self.bi("Total including VAT", &ar.total) }, d.total, true));

        if self.thermal {
            self.receipt_lines(h, d, has_class, has_qty, &totals);
            return;
        }
        // Columns: (header lines, numeric, cells).
        let lang = self.lang;
        let bil = |en: &str, a: &str| match lang {
            Lang::English => vec![en.to_owned()],
            Lang::Bilingual => vec![en.to_owned(), a.to_owned()],
            Lang::Arabic => vec![a.to_owned()],
        };
        let mut cols: Vec<(Vec<String>, bool, Vec<String>)> = vec![
            (vec![if lang == Lang::Arabic { "م" } else { "#" }.into()], true, (1..=d.lines.len()).map(|i| self.num(&i.to_string())).collect()),
            (bil("Description", &ar.description), false, d.lines.iter().map(|l| l.description.clone()).collect()),
        ];
        if has_class {
            let head = if english { vec![crate::class_heading(d).into()] } else { bil("Tax class", &ar.tax_class) };
            cols.push((head, false, d.lines.iter().map(|l| l.tax_class.clone().unwrap_or_default()).collect()));
        }
        if has_qty {
            cols.push((bil("Quantity", &ar.quantity), true, d.lines.iter().map(|l| self.num(l.qty.as_deref().unwrap_or_default())).collect()));
            let head = if english { vec!["Rate".into()] } else { bil("Unit price", &ar.unit_price) };
            cols.push((head, true, d.lines.iter().map(|l| l.rate.map(|r| self.amount(r, h)).unwrap_or_default()).collect()));
        }
        let mut amounts: Vec<String> = d.lines.iter().map(|l| self.amount(l.amount, h)).collect();
        let total_amounts: Vec<String> = totals.iter().map(|t| self.amount(t.1, h)).collect();
        cols.push((bil("Amount", &ar.amount), true, amounts.clone()));
        amounts.extend(total_amounts.iter().cloned());

        let cw = self.content_width();
        let size = st.base;
        let pad = st.pad;
        let caps = |i: usize, n: usize| -> f32 {
            if i == 0 {
                0.07
            } else if i == n - 1 {
                0.24
            } else {
                0.18
            }
        };
        let ncols = cols.len();
        let mut widths: Vec<f32> = cols
            .iter()
            .enumerate()
            .map(|(i, (head, _, cells))| {
                if i == 1 {
                    return 0.0;
                }
                let content = if i == ncols - 1 { &amounts } else { cells };
                let w = head.iter().map(|s| self.width(s, size)).chain(content.iter().map(|s| self.width(s, size))).fold(0.0, f32::max) + 2.0 * pad + 1.0;
                w.min(cw * caps(i, ncols))
            })
            .collect();
        let others: f32 = widths.iter().sum();
        if cw - others < cw * 0.3 {
            let k = (cw * 0.7) / others;
            for w in widths.iter_mut() {
                *w *= k;
            }
        }
        widths[1] = cw - widths.iter().sum::<f32>();
        let xs: Vec<f32> = widths.iter().scan(self.left, |x, w| {
            let at = *x;
            *x += w;
            Some(at)
        }).collect();

        let header_lines: Vec<Vec<String>> = cols.iter().enumerate().map(|(i, (head, _, _))| head.iter().flat_map(|s| self.wrap(s, size, widths[i] - 2.0 * pad)).collect()).collect();
        let head_h = header_lines.iter().map(Vec::len).max().unwrap_or(1) as f32 * lh(size) + 2.0 * pad;
        let numeric: Vec<bool> = cols.iter().map(|c| c.1).collect();
        let draw_header = |me: &mut Layout| {
            let top = me.y;
            let modern = st.template == Template::Modern;
            if modern {
                me.op(Op::Rect { x: me.left, y: top, w: cw, h: head_h, fill: Some(st.accent), stroke: None });
            }
            for (i, lines) in header_lines.iter().enumerate() {
                let mut y = top + pad;
                for line in lines {
                    // Mirrored later on a right-to-left page, where every cell is right-aligned.
                    let (x, align) = if numeric[i] && !me.rtl { (xs[i] + widths[i] - pad, Align::Right) } else { (xs[i] + pad, Align::Left) };
                    me.text(x, y + size, line, size, true, if modern { WHITE } else { BLACK }, align);
                    y += lh(size);
                }
            }
            me.y = top + head_h;
            if !modern {
                me.rule(st.accent, 1.0);
            }
        };
        self.ensure(head_h + lh(size) + 2.0 * pad);
        draw_header(self);
        let draw_row = |me: &mut Layout, cells: Vec<(Vec<String>, bool, bool)>, total: bool| {
            let row_h = cells.iter().map(|c| c.0.len()).max().unwrap_or(1).max(1) as f32 * lh(size) + 2.0 * pad;
            if me.ensure(row_h) {
                draw_header(me);
            }
            let top = me.y;
            if total {
                me.rule(BLACK, 1.0);
            }
            for (i, (lines, num, bold)) in cells.into_iter().enumerate() {
                let mut y = top + pad;
                for line in lines {
                    let rtl = !num && !me.rtl && shape::starts_rtl(&line);
                    let right = if me.rtl { false } else { num || rtl };
                    let (x, align) = if right { (xs[i] + widths[i] - pad, Align::Right) } else { (xs[i] + pad, Align::Left) };
                    me.text(x, y + size, &line, size, bold, BLACK, align);
                    y += lh(size);
                }
            }
            me.y = top + row_h;
            me.rule(if total { BLACK } else { RULE }, if total { 1.0 } else { 0.4 });
        };
        for r in 0..d.lines.len() {
            let cells = cols.iter().enumerate().map(|(i, (_, num, cells))| (self.wrap(&cells[r], size, widths[i] - 2.0 * pad), *num, false)).collect();
            draw_row(self, cells, false);
        }
        let last = ncols - 1;
        for (t, amount) in totals.iter().zip(&total_amounts) {
            let cells = (0..ncols)
                .map(|i| {
                    if i == 1 {
                        (self.wrap(&t.0, size, widths[1] - 2.0 * pad), false, t.2)
                    } else if i == last {
                        (vec![amount.clone()], true, t.2)
                    } else {
                        (Vec::new(), numeric[i], false)
                    }
                })
                .collect();
            draw_row(self, cells, t.2);
        }
        self.y += st.gap;
    }

    fn receipt_lines(&mut self, h: &Header, d: &InvoiceDoc, has_class: bool, has_qty: bool, totals: &[(String, Amount, bool)]) {
        let st = self.st;
        let size = st.base;
        let cw = self.content_width();
        let (l, r) = (self.left, self.w - self.right);
        for line in &d.lines {
            self.para(l, cw, &line.description, size, true, BLACK, Align::Left);
            let mut left = String::new();
            if has_qty && let Some(q) = &line.qty {
                left = format!("{} × {}", self.num(q), line.rate.map(|x| self.amount(x, h)).unwrap_or_default());
            }
            if has_class && let Some(c) = &line.tax_class {
                left = if left.is_empty() { c.clone() } else { format!("{left}  {c}") };
            }
            let amount = self.amount(line.amount, h);
            let y = self.y + size;
            self.text(l, y, &left, size, false, GREY, Align::Left);
            self.text(r, y, &amount, size, false, BLACK, Align::Right);
            self.y += lh(size) + 1.0;
        }
        self.op(Op::Line { x1: l, y1: self.y, x2: r, y2: self.y, colour: RULE, width: 0.4 });
        self.y += 2.0;
        for (label, a, bold) in totals {
            if *bold {
                self.op(Op::Line { x1: l, y1: self.y, x2: r, y2: self.y, colour: BLACK, width: 0.8 });
                self.y += 1.5;
            }
            let amount = self.amount(*a, h);
            let aw = self.width(&amount, size) + 6.0;
            let start = self.y;
            self.para(l, cw - aw, label, size, *bold, BLACK, Align::Left);
            self.text(r, start + size, &amount, size, *bold, BLACK, Align::Right);
        }
        self.y += st.gap;
    }

    fn closing(&mut self, h: &Header, d: &InvoiceDoc, look: &Look) {
        let st = self.st;
        let ar = ArabicLabels::default();
        let cw = self.content_width();
        let l = self.left;
        let design = look.design;
        if design.show_amount_in_words && !d.total_in_words.trim().is_empty() {
            let words_ar = d.total_in_words_ar.as_deref().filter(|w| !w.trim().is_empty());
            // Arabic pages print the Arabic words alone when there are some.
            if !(self.lang == Lang::Arabic && words_ar.is_some()) {
                let s = format!("{}: {}", self.bi("Amount in words", &ar.amount_in_words), self.val(&d.total_in_words));
                self.para(l, cw, &s, st.base, false, BLACK, Align::Left);
            }
            // Bilingual: the Arabic words under the English, right to left.
            if self.lang != Lang::English && let Some(w) = words_ar {
                self.para(l, cw, &format!("{}: {w}", ar.amount_in_words), st.base, false, BLACK, Align::Left);
            }
            self.y += 3.0;
        }
        if !d.narration.trim().is_empty() {
            self.para(l, cw, &d.narration, st.base, false, BLACK, Align::Left);
            self.y += 3.0;
        }
        for n in &d.legal_notes {
            self.para(l, cw, &n.text, st.base, true, BLACK, Align::Left);
            if self.lang != Lang::English && let Some(a) = n.arabic.as_deref().filter(|a| !a.trim().is_empty()) {
                self.para(l, cw, a, st.base, true, BLACK, Align::Left);
            }
            self.y += 3.0;
        }
        self.y += st.gap / 2.0;
        if look.trade_document && !design.bank.is_empty() {
            let b = &design.bank;
            let labels = look.bank_labels;
            let mut rows: Vec<(String, String)> = Vec::new();
            let mut push = |label: String, v: &str| {
                if !v.trim().is_empty() {
                    rows.push((label, v.trim().to_owned()));
                }
            };
            push(self.nl("Account name"), &b.account_name);
            push(self.nl("Bank"), &b.bank_name);
            push(self.nl("Branch"), &b.branch);
            push(self.nl(labels.account_number), &b.account_number);
            push(self.nl("IBAN"), &b.iban);
            push(labels.local_code.unwrap_or("Bank code").to_owned(), &b.local_code);
            push(self.nl("SWIFT / BIC"), &b.swift);
            let heading = self.nl("Bank details");
            self.kv(&heading, &rows);
        }
        if look.trade_document {
            for (title, text) in [("Terms and conditions", &design.terms), ("Declaration", &design.declaration)] {
                if text.trim().is_empty() {
                    continue;
                }
                let heading = self.nl(title);
                self.ensure(lh(st.base) + lh(st.small) * 2.0);
                self.text(l, self.y + st.base, &heading, st.base, true, st.accent, Align::Left);
                self.y += lh(st.base);
                self.para(l, cw, text, st.small, false, BLACK, Align::Left);
                self.y += st.gap;
            }
        }
        if look.show_signatory() {
            let sig_h = 15.0 * MM;
            self.ensure(lh(st.base) * 2.0 + sig_h + 4.0);
            let (x, align) = if self.thermal { (self.w / 2.0, Align::Centre) } else { (self.w - self.right, Align::Right) };
            let for_company = if self.lang == Lang::Arabic { format!("عن {}", self.val(h.company)) } else { format!("For {}", h.company) };
            self.text(x, self.y + st.base, &for_company, st.base, true, BLACK, align);
            self.y += lh(st.base) + 2.0;
            let drawn = look.signature.and_then(|s| self.image(s, x, 40.0 * MM, sig_h, align));
            self.y += drawn.map_or(sig_h * 0.8, |(_, ih)| ih) + 2.0;
            let label = self.bi("Authorised signatory", &ar.signatory);
            self.text(x, self.y + st.base, &label, st.base, false, BLACK, align);
            self.y += lh(st.base) + st.gap;
        }
        if !design.footer.trim().is_empty() {
            self.para(l, cw, &design.footer, st.small, false, GREY, Align::Centre);
        }
    }
}

/// Lay out every document.
pub(crate) fn layout(h: &Header, docs: &[PdfDoc]) -> Layout {
    let mut l = Layout::new();
    for d in docs {
        l.document(h, d);
    }
    l.display.title = docs.first().map(|d| format!("{} {}", d.doc.title, d.doc.number)).unwrap_or_default();
    l
}

//! Text for the PDF writer: one line in, positioned glyphs out.
//!
//! A PDF shows glyphs where it is told to, so each line is turned into
//! glyphs before it is drawn:
//!
//! 1. The Unicode Bidirectional Algorithm (`unicode-bidi`, UAX #9 with
//!    isolates) splits the line into runs in visual order. The paragraph
//!    direction is the first strong letter's (outside isolates), else the
//!    page's. The layout wraps the Arabic side of "English / Arabic" labels
//!    in an RLI…PDI isolate, as the HTML print does with `dir="rtl"` spans,
//!    so "VAT number / الرقم الضريبي: 300…" keeps its colon and number after
//!    the Arabic.
//! 2. Each run is split by script (`unicode-script`; common characters
//!    such as spaces and digits join their neighbours) and by font: the
//!    font of the script, else the fallbacks in `font.rs`.
//! 3. Each piece is shaped with rustybuzz (a port of HarfBuzz): Indic
//!    conjuncts, matra reordering and reph, Arabic joining, lam-alef and
//!    mark placement, Hebrew points, kerning; bracket mirroring in
//!    right-to-left runs. Right-to-left pieces come out in visual order.
//!
//! Each glyph carries its share of its cluster's text, which the writer
//! puts in the ToUnicode map so the PDF's text copies and searches as it
//! was typed: the glyphs of a cluster, in reading order, spell it (a
//! reordered i-matra glyph stands for the consonant, the consonant for the
//! matra, so कि still reads कि).

use rustybuzz::{BufferFlags, Direction, UnicodeBuffer};
use unicode_bidi::{BidiClass, Level, ParagraphBidiInfo, bidi_class};
use unicode_script::{Script, UnicodeScript};

use super::font::{self, Cjk, FontId};

/// One glyph to draw.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Glyph {
    pub font: FontId,
    pub gid: u16,
    /// Advance and offsets, in thousandths of the font size.
    pub advance: f32,
    pub dx: f32,
    pub dy: f32,
    /// What the glyph stands for: its share of its cluster's text (the
    /// glyphs of a cluster, in reading order, spell the cluster).
    pub text: String,
    /// Where its cluster starts in the line (bytes).
    pub cluster: usize,
}

/// A shaped line: glyphs left to right and the width, in thousandths of the
/// font size.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Line {
    pub glyphs: Vec<Glyph>,
    pub width: f32,
}

/// A line's direction when it has no strong letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum Dir {
    #[default]
    Ltr,
    Rtl,
}

/// Invisible formatting characters: bidi controls, joiners, the BOM.
pub(crate) fn is_format(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}' | '\u{061C}')
}

/// `s` without its invisible formatting characters (what a line says).
pub(crate) fn plain(s: &str) -> String {
    s.chars().filter(|&c| !is_format(c)).collect()
}

/// The direction of the first strong letter outside isolates, if any.
pub(crate) fn strong_dir(s: &str) -> Option<Dir> {
    let mut depth = 0usize;
    for c in s.chars() {
        match bidi_class(c) {
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => depth += 1,
            BidiClass::PDI => depth = depth.saturating_sub(1),
            BidiClass::L if depth == 0 => return Some(Dir::Ltr),
            BidiClass::R | BidiClass::AL if depth == 0 => return Some(Dir::Rtl),
            _ => {}
        }
    }
    None
}

/// Whether a line reads right to left (its first strong letter is).
pub(crate) fn starts_rtl(s: &str) -> bool {
    strong_dir(s) == Some(Dir::Rtl)
}

/// Controls as spaces, keeping every byte offset (C1 controls are two bytes,
/// as is a no-break space).
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{80}'..='\u{9F}' => '\u{A0}',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect()
}

/// Characters that belong to the cluster before them whatever their script.
fn joins_previous(c: char, script: Script) -> bool {
    script == Script::Inherited || matches!(c, '\u{200C}' | '\u{200D}' | '\u{FE00}'..='\u{FE0F}')
}

/// The CJK fonts to prefer on this line.
fn cjk_hint(s: &str) -> Cjk {
    let mut hint = Cjk::Chinese;
    for c in s.chars() {
        match c.script() {
            Script::Hiragana | Script::Katakana => return Cjk::Japanese,
            Script::Hangul => hint = Cjk::Korean,
            _ => {}
        }
    }
    hint
}

/// A piece of a run in one font and script: byte range in the line.
struct Piece {
    start: usize,
    end: usize,
    /// `None`: no font has these characters; they print as "?".
    font: Option<FontId>,
    script: Script,
}

/// Split a run (logical order) into pieces by script and font.
fn pieces(text: &str, start: usize, end: usize, cjk: Cjk) -> Vec<Piece> {
    let chars: Vec<(usize, char)> = text[start..end].char_indices().map(|(i, c)| (start + i, c)).collect();
    // Scripts, with common and inherited characters taking their neighbours'.
    let raw: Vec<Script> = chars.iter().map(|&(_, c)| c.script()).collect();
    let real = |s: Script| !matches!(s, Script::Common | Script::Inherited | Script::Unknown);
    let first = raw.iter().copied().find(|&s| real(s)).unwrap_or(Script::Common);
    let mut scripts = Vec::with_capacity(raw.len());
    let mut last = first;
    for &s in &raw {
        if real(s) {
            last = s;
        }
        scripts.push(last);
    }
    let mut out: Vec<Piece> = Vec::new();
    let mut prev_font: Option<Option<FontId>> = None;
    for (k, &(i, c)) in chars.iter().enumerate() {
        let script = scripts[k];
        let font = match prev_font {
            Some(f) if joins_previous(c, raw[k]) => f,
            Some(Some(f)) if !real(raw[k]) && font::get(f).has(c) => Some(f),
            _ => font::font_for(c, if real(raw[k]) { raw[k] } else { script }, cjk),
        };
        prev_font = Some(font);
        let end = i + c.len_utf8();
        match out.last_mut() {
            Some(p) if p.font == font && p.script == script && font.is_some() => p.end = end,
            _ => out.push(Piece { start: i, end, font, script }),
        }
    }
    out
}

fn rb_script(s: Script) -> Option<rustybuzz::Script> {
    if matches!(s, Script::Common | Script::Inherited | Script::Unknown) {
        return None;
    }
    let tag = s.short_name().as_bytes();
    let tag: [u8; 4] = tag.try_into().ok()?;
    rustybuzz::Script::from_iso15924_tag(rustybuzz::ttf_parser::Tag::from_bytes(&tag))
}

/// Shape one piece; glyphs in visual order.
fn shape_piece(text: &str, p: &Piece, rtl: bool, out: &mut Vec<Glyph>) {
    let s = &text[p.start..p.end];
    let Some(id) = p.font else {
        // No font: a "?" for each character, copying as the character.
        let f = font::default_font();
        let q = f.glyph('?').unwrap_or(0);
        let mut glyphs: Vec<Glyph> = s
            .char_indices()
            .filter(|&(_, c)| !is_format(c))
            .map(|(i, c)| Glyph { font: f.id, gid: q, advance: f.advance_1000(q) as f32, dx: 0.0, dy: 0.0, text: c.to_string(), cluster: p.start + i })
            .collect();
        if rtl {
            glyphs.reverse();
        }
        out.extend(glyphs);
        return;
    };
    let f = font::get(id);
    let mut buf = UnicodeBuffer::new();
    buf.push_str(s);
    buf.set_direction(if rtl { Direction::RightToLeft } else { Direction::LeftToRight });
    if let Some(sc) = rb_script(p.script) {
        buf.set_script(sc);
    }
    buf.guess_segment_properties();
    buf.set_flags(BufferFlags::REMOVE_DEFAULT_IGNORABLES);
    let shaped = rustybuzz::shape(&f.face, &[], buf);
    let infos = shaped.glyph_infos();
    let positions = shaped.glyph_positions();
    let mut starts: Vec<usize> = infos.iter().map(|g| g.cluster as usize).collect();
    starts.sort_unstable();
    starts.dedup();
    let end_of = |c: usize| starts.iter().copied().find(|&x| x > c).unwrap_or(s.len());
    let first = out.len();
    for (g, pos) in infos.iter().zip(positions) {
        out.push(Glyph {
            font: id,
            gid: u16::try_from(g.glyph_id).unwrap_or(0),
            advance: f.to_1000(pos.x_advance as f32),
            dx: f.to_1000(pos.x_offset as f32),
            dy: f.to_1000(pos.y_offset as f32),
            text: String::new(),
            cluster: g.cluster as usize,
        });
    }
    // The cluster's characters go to its glyphs that advance (a reader
    // takes a glyph with no text for a gap, and puts a space there), in
    // reading order: left to right, or right to left in a right-to-left run.
    let mut i = first;
    while i < out.len() {
        let c = out[i].cluster;
        let mut j = i;
        while j < out.len() && out[j].cluster == c {
            j += 1;
        }
        let chars: Vec<char> = plain(s.get(c..end_of(c)).unwrap_or_default()).chars().collect();
        let mut takers: Vec<usize> = (i..j).filter(|&k| out[k].advance > 0.0).collect();
        if takers.is_empty() {
            takers.push(i);
        }
        if rtl {
            takers.reverse();
        }
        // The first taker gets what the others (one character each) leave.
        let rest = takers.len().saturating_sub(1).min(chars.len().saturating_sub(1));
        let head = chars.len() - rest;
        for (n, &k) in takers.iter().enumerate() {
            out[k].text = match n {
                0 => chars[..head].iter().collect(),
                n if n <= rest => chars[head + n - 1].to_string(),
                _ => String::new(),
            };
        }
        i = j;
    }
    for g in &mut out[first..] {
        g.cluster += p.start;
    }
}

/// One line as glyphs, left to right. `dir` is the direction of a line with
/// no strong letter (the page's).
pub(crate) fn shape(s: &str, dir: Dir) -> Line {
    let text = clean(s);
    if text.is_empty() {
        return Line::default();
    }
    let rtl_para = strong_dir(&text).unwrap_or(dir) == Dir::Rtl;
    let level = if rtl_para { Level::rtl() } else { Level::ltr() };
    let cjk = cjk_hint(&text);
    let bidi = ParagraphBidiInfo::new(&text, Some(level));
    let (_, runs) = bidi.visual_runs(0..text.len());
    let mut glyphs = Vec::new();
    for run in runs {
        let rtl = bidi.levels[run.start].is_rtl();
        let mut ps = pieces(&text, run.start, run.end, cjk);
        if rtl {
            ps.reverse();
        }
        for p in &ps {
            shape_piece(&text, p, rtl, &mut glyphs);
        }
    }
    let width = glyphs.iter().map(|g| g.advance).sum();
    Line { glyphs, width }
}

/// Where a word may be broken when it is too long for its line: the start
/// of each cluster (byte offsets, in order), so a conjunct or a letter with
/// its marks stays whole.
pub(crate) fn breaks(s: &str) -> Vec<usize> {
    let mut b: Vec<usize> = shape(s, Dir::Ltr).glyphs.iter().map(|g| g.cluster).collect();
    b.sort_unstable();
    b.dedup();
    b.retain(|&i| i > 0 && s.is_char_boundary(i));
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The glyph names would be nicer, but ids are what the font gives.
    fn ids(l: &Line) -> Vec<u16> {
        l.glyphs.iter().map(|g| g.gid).collect()
    }

    fn texts(l: &Line) -> Vec<&str> {
        l.glyphs.iter().map(|g| g.text.as_str()).collect()
    }

    fn font_of(l: &Line) -> &'static str {
        &font::get(l.glyphs[0].font).name
    }

    /// The text a PDF reader gets back, in visual order (what ToUnicode says).
    fn visual_text(l: &Line) -> String {
        l.glyphs.iter().map(|g| g.text.as_str()).collect()
    }

    /// The glyph a single character maps to in a line's font, unshaped.
    fn nominal(l: &Line, c: char) -> u16 {
        font::get(l.glyphs[0].font).glyph(c).unwrap()
    }

    #[test]
    fn devanagari_conjuncts_reorder_and_reph() {
        // क्षत्रिय: क्ष is one conjunct glyph, त्र another, ि moves before र… (the i-matra of त्रि).
        let l = shape("क्षत्रिय", Dir::Ltr);
        assert_eq!(font_of(&l), "NotoSansDevanagari-Regular");
        assert!(l.glyphs.len() < "क्षत्रिय".chars().count(), "conjuncts: {:?}", ids(&l));
        assert!(!ids(&l).contains(&nominal(&l, '\u{094D}')), "no visible virama: {:?}", ids(&l));
        let i_matra = l.glyphs.iter().position(|g| font::get(g.font).glyph('\u{093F}') == Some(g.gid) || g.text.contains('\u{093F}')).unwrap();
        let ya = l.glyphs.iter().position(|g| g.gid == nominal(&l, 'य')).unwrap();
        assert!(i_matra < ya, "the i-matra is drawn before the cluster it ends");
        assert_eq!(visual_text(&l), "क्षत्रिय", "clusters copy back as typed");
        // श्री: श + ् + र + ी → श्र conjunct then ी.
        let l = shape("श्री", Dir::Ltr);
        assert_eq!(l.glyphs.len(), 2, "{:?}", ids(&l));
        assert_eq!(texts(&l), ["श्र", "ी"], "one cluster, its text over the glyphs that advance");
        // Reph: र्म — the ra becomes a mark over म, drawn after it.
        let l = shape("धर्म", Dir::Ltr);
        assert_eq!(visual_text(&l), "धर्म");
        assert!(!ids(&l).contains(&nominal(&l, 'र')), "ra became a reph: {:?}", ids(&l));
        // Marathi uses the same font and shaper.
        assert_eq!(font_of(&shape("महाराष्ट्र", Dir::Ltr)), "NotoSansDevanagari-Regular");
    }

    #[test]
    fn every_indian_script_shapes_with_its_noto_font() {
        for (word, font) in [
            ("શ્રી", "NotoSansGujarati-Regular"),
            ("ஸ்ரீ", "NotoSansTamil-Regular"),
            ("శ్రీ", "NotoSansTelugu-Regular"),
            ("ಶ್ರೀ", "NotoSansKannada-Regular"),
            ("ശ്രീ", "NotoSansMalayalam-Regular"),
            ("শ্রী", "NotoSansBengali-Regular"),
            ("ਸ਼੍ਰੀ", "NotoSansGurmukhi-Regular"),
            ("ଶ୍ରୀ", "NotoSansOriya-Regular"),
        ] {
            let l = shape(word, Dir::Ltr);
            assert_eq!(font_of(&l), font, "{word}");
            assert!(l.glyphs.iter().all(|g| g.gid != 0), "{word}: no missing glyphs");
            assert!(l.glyphs.len() < word.chars().count(), "{word}: shaped into fewer glyphs {:?}", ids(&l));
            assert_eq!(visual_text(&l), word, "{word} copies back");
            assert!(l.width > 300.0);
        }
        // Tamil ஸ்ரீ is the shri ligature: one glyph.
        assert_eq!(shape("ஸ்ரீ", Dir::Ltr).glyphs.len(), 1);
        // Gujarati શ્રી: the conjunct and the ii-matra.
        assert_eq!(shape("શ્રી", Dir::Ltr).glyphs.len(), 2);
    }

    #[test]
    fn arabic_joins_with_lam_alef_and_reads_right_to_left() {
        assert!(starts_rtl("لا إله"));
        // لا إله: Noto Sans Arabic draws lam-alef as a required ligature of
        // two contextual glyphs (rlig), not the separate lam and alef.
        let l = shape("لا إله", Dir::Ltr);
        assert_eq!(font_of(&l), "NotoSansArabic-Regular");
        let f = font::get(l.glyphs[0].font);
        let name = |g: &Glyph| f.face.glyph_name(rustybuzz::ttf_parser::GlyphId(g.gid)).unwrap_or_default().to_owned();
        let names: Vec<String> = l.glyphs.iter().map(name).collect();
        // Visual order: the first word (لا) is drawn last, at the right.
        assert_eq!(names[names.len() - 2..], ["uni0627.fina.rlig", "uni0644.init.rlig"], "{names:?}");
        assert_eq!(texts(&l)[l.glyphs.len() - 2..], ["ا", "ل"]);
        // إله: heh final, lam initial, alef with the hamza below as a mark.
        assert_eq!(names[..2], ["uni0647.fina", "uni0644.init"], "{names:?}");
        assert!(names.contains(&"uni0655".to_owned()), "hamza below placed as a mark: {names:?}");
        let lam_init = shape("لم", Dir::Ltr).glyphs[1].gid;
        assert_ne!(lam_init, l.glyphs[l.glyphs.len() - 1].gid, "lam before alef is the ligature form");
        // سلام: the medial lam-alef ligature.
        let names: Vec<String> = shape("سلام", Dir::Ltr).glyphs.iter().map(name).collect();
        assert_eq!(names, ["uni0645", "uni0627.fina.rlig.2", "uni0644.medi.rlig", "uni0633.init"]);
        // Joining: a medial beh differs from an isolated one.
        let alone = shape("ب", Dir::Ltr).glyphs[0].gid;
        let medial = shape("ببب", Dir::Ltr).glyphs[1].gid;
        assert_ne!(alone, medial);
        // Read back in reverse, the clusters give the text as typed.
        let back: String = l.glyphs.iter().rev().map(|g| g.text.as_str()).collect();
        assert_eq!(back, "لا إله");
    }

    #[test]
    fn mixed_lines_follow_the_bidi_algorithm() {
        // An English line: Invoice, then the Arabic word with the number
        // that follows it (UAX #9: a number after Arabic letters belongs to them).
        let l = shape("Invoice فاتورة 123", Dir::Ltr);
        let v = visual_text(&l);
        assert!(v.starts_with("Invoice "), "{v}");
        let arabic_at = v.find('ة').unwrap();
        let number_at = v.find("123").unwrap();
        assert!(number_at < arabic_at, "123 is left of the Arabic: {v}");
        // Latin in DejaVu, Arabic in Noto Sans Arabic.
        let names: Vec<&str> = l.glyphs.iter().map(|g| font::get(g.font).name.as_str()).collect();
        assert_eq!(names[0], "DejaVuSans");
        assert!(names.contains(&"NotoSansArabic-Regular"));
        // An isolated Arabic label keeps the colon and number after it.
        let l = shape("VAT number / \u{2067}الرقم الضريبي\u{2069}: 300075588700003", Dir::Ltr);
        assert!(visual_text(&l).ends_with(": 300075588700003"), "{}", visual_text(&l));
        // Numbers inside Arabic text keep their digits' order; amounts stay whole.
        let l = shape("المبلغ 1,150.00 ريال", Dir::Ltr);
        let v = visual_text(&l);
        assert!(v.contains("1,150.00"), "{v}");
        assert!(v.find('ل').unwrap() < v.find("1,150.00").unwrap(), "ريال (last word) is drawn first, at the left: {v}");
        // Brackets mirror in a right-to-left run.
        let l = shape("(ب)", Dir::Rtl);
        let f = font::get(l.glyphs[0].font);
        assert_eq!(l.glyphs[0].gid, f.glyph('(').unwrap(), "the closing bracket mirrored to ( on the left");
        // No strong letter: the page's direction.
        assert!(shape("123", Dir::Rtl).width > 0.0);
    }

    #[test]
    fn hebrew_reads_right_to_left_in_dejavu() {
        let l = shape("שלום", Dir::Ltr);
        assert_eq!(font_of(&l), "DejaVuSans");
        assert_eq!(texts(&l), ["ם", "ו", "ל", "ש"]);
    }

    #[test]
    fn missing_characters_print_as_question_marks_but_copy_as_themselves() {
        let l = shape("a\u{0378}b", Dir::Ltr);
        let f = font::default_font();
        assert_eq!(l.glyphs[1].gid, f.glyph('?').unwrap());
        assert_eq!(l.glyphs[1].text, "\u{0378}");
        assert_eq!(visual_text(&l), "a\u{0378}b");
    }

    #[test]
    fn cjk_uses_a_system_font_when_there_is_one() {
        let l = shape("发票 請求書 영수증", Dir::Ltr);
        if font::system::has_cjk() {
            assert!(l.glyphs.iter().all(|g| g.gid != font::default_font().glyph('?').unwrap() || g.text == " "), "{:?}", texts(&l));
            assert_eq!(visual_text(&l), "发票 請求書 영수증");
        } else {
            assert_eq!(visual_text(&l), "发票 請求書 영수증", "printed as ?, copied as typed");
        }
    }

    #[test]
    fn controls_and_format_marks_draw_nothing() {
        let l = shape("a\u{200E}b\tc", Dir::Ltr);
        assert_eq!(visual_text(&l), "ab c");
        assert_eq!(plain("x\u{2067}y\u{2069}"), "xy");
        // A long word breaks between clusters only.
        assert_eq!(breaks("क्षत्रिय"), ["क्ष".len(), "क्षत्रि".len()]);
        assert_eq!(breaks("abc"), [1, 2]);
    }
}

//! The fonts of the PDF export: the bundled ones, the system's fallback
//! fonts, and the folders of fonts the app adds (font packs).
//!
//! Bundled (`assets/`, deflated into the binary by `build.rs` and inflated
//! the first time a PDF needs each one):
//!
//! - DejaVu Sans 2.37 (Bitstream Vera licence with public-domain DejaVu
//!   changes, `assets/DejaVu-LICENSE.txt`): Latin, Greek, Cyrillic, Hebrew,
//!   the currency signs (₹ € £ …) and the symbols; the default font.
//! - Noto Sans Arabic, Bengali, Devanagari (Hindi, Marathi), Gujarati,
//!   Gurmukhi (Punjabi), Kannada, Malayalam, Oriya, Tamil and Telugu,
//!   Regular, unhinted (SIL Open Font License 1.1, `assets/noto/OFL.txt`).
//!
//! The shaper (`shape.rs`) picks a font for every character: the bundled
//! font of its script, else DejaVu Sans, else any bundled font that has it,
//! else a font from the folders added with [`add_font_dir`], else a system
//! font of the script ([`system`]: CJK, Thai, Sinhala…). Bundled fonts come
//! first, so a PDF looks the same on every machine for every script they
//! cover; system fonts only fill in what they lack.
//!
//! CJK fonts are not bundled (Noto Sans CJK is about 16 MB a weight). Every
//! platform the app runs on ships one, found by family name:
//! - Windows: Microsoft YaHei, Yu Gothic, Malgun Gothic (`%WINDIR%\Fonts`);
//! - macOS: PingFang, Hiragino, Apple SD Gothic Neo (`/System/Library/Fonts`
//!   and its downloadable-asset folders);
//! - iOS: Hiragino Kaku Gothic and Apple SD Gothic Neo are plain files in
//!   `/System/Library/Fonts/Core` (PingFang is not a file there). They are
//!   in the simulator's runtime; that the device sandbox lets an app read
//!   them is expected but not yet checked on a device;
//! - Android: `/system/fonts` (NotoSansCJK), read directly, since fontdb
//!   has no Android folders;
//! - Linux: fontconfig's folders (Noto Sans CJK, WenQuanYi, Droid Sans
//!   Fallback on most desktops).
//!
//! Where none is found (a bare Linux install, or an iOS device that refuses
//! the read), the fonts in the folder given to [`add_font_dir`] (the app's
//! data folder `fonts/`) are used; the app never downloads one by itself. A
//! character no font has prints as "?" (and still copies as itself).
//!
//! Fonts are read with `ttf-parser` (through rustybuzz) and subset per PDF
//! with `subsetter` (`writer.rs`); TrueType and CFF outlines both embed.
//! Colour-bitmap fonts (emoji) and CFF2 fonts are skipped.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rustybuzz::ttf_parser::{self, GlyphId};
use unicode_script::Script;

/// A loaded font, by its place in the process-wide list.
pub(crate) type FontId = u16;

/// A font ready to shape with and to embed. Every font is loaded once per
/// process and kept (its data is static from then on).
pub(crate) struct Font {
    pub id: FontId,
    /// The PostScript name, letters and digits only ("NotoSansDevanagari-Regular").
    pub name: String,
    pub data: &'static [u8],
    /// The face in a collection (`.ttc`).
    pub index: u32,
    pub face: rustybuzz::Face<'static>,
    pub upem: u16,
    pub ascent: i16,
    pub descent: i16,
    pub cap_height: i16,
    pub bbox: [i16; 4],
    /// CFF outlines (embedded as CIDFontType0) rather than TrueType.
    pub cff: bool,
}

impl Font {
    /// The font in `data`, when it can be shaped with and embedded.
    fn new(id: FontId, data: &'static [u8], index: u32, name_hint: Option<&str>) -> Option<Font> {
        let face = rustybuzz::Face::from_slice(data, index)?;
        let t = face.tables();
        // Colour bitmaps (emoji) have no outlines to embed; CFF2 isn't subset.
        if t.sbix.is_some() || t.cbdt.is_some() || (t.glyf.is_none() && t.cff.is_none()) {
            return None;
        }
        subsetter::subset(data, index, &subsetter::GlyphRemapper::new()).ok()?;
        let name = face
            .names()
            .into_iter()
            .filter(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
            .find_map(|n| n.to_string())
            .map(|n| n.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>())
            .filter(|n| !n.is_empty())
            .or_else(|| name_hint.map(|n| n.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>()).filter(|n| !n.is_empty()))
            .unwrap_or_else(|| format!("Font{id}"));
        let b = face.global_bounding_box();
        Some(Font {
            id,
            name,
            data,
            index,
            upem: u16::try_from(face.units_per_em()).unwrap_or(1000),
            ascent: face.ascender(),
            descent: face.descender(),
            cap_height: face.capital_height().unwrap_or_else(|| face.ascender()),
            bbox: [b.x_min, b.y_min, b.x_max, b.y_max],
            cff: t.cff.is_some() && t.glyf.is_none(),
            face,
        })
    }

    /// The glyph for a character, if the font has one.
    pub fn glyph(&self, c: char) -> Option<u16> {
        self.face.glyph_index(c).map(|g| g.0).filter(|&g| g != 0)
    }

    pub fn has(&self, c: char) -> bool {
        self.glyph(c).is_some()
    }

    /// A length in font units, in thousandths of the font size.
    pub fn to_1000(&self, v: f32) -> f32 {
        v * 1000.0 / f32::from(self.upem.max(1))
    }

    /// Advance width in thousandths of the font size (PDF glyph space).
    pub fn advance_1000(&self, gid: u16) -> i32 {
        self.to_1000(f32::from(self.face.glyph_hor_advance(GlyphId(gid)).unwrap_or(0))).round() as i32
    }
}

// ------------------------------------------------------------ the list

fn fonts() -> &'static Mutex<Vec<&'static Font>> {
    static FONTS: OnceLock<Mutex<Vec<&'static Font>>> = OnceLock::new();
    FONTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Load a font into the process-wide list; `None` when it can't be used.
fn load(data: &'static [u8], index: u32, name_hint: Option<&str>) -> Option<FontId> {
    let mut list = fonts().lock().unwrap_or_else(|e| e.into_inner());
    let id = FontId::try_from(list.len()).ok()?;
    let f = Font::new(id, data, index, name_hint)?;
    list.push(Box::leak(Box::new(f)));
    Some(id)
}

/// A loaded font.
pub(crate) fn get(id: FontId) -> &'static Font {
    let list = fonts().lock().unwrap_or_else(|e| e.into_inner());
    list[usize::from(id)]
}

// ------------------------------------------------------------ bundled

/// A font compiled into the binary (deflated by `build.rs`).
struct Bundled {
    data: &'static [u8],
    scripts: &'static [Script],
}

macro_rules! bundled {
    ($file:literal, $scripts:expr) => {
        Bundled { data: include_bytes!(concat!(env!("OUT_DIR"), "/", $file, ".z")), scripts: $scripts }
    };
}

/// DejaVu Sans first: the default.
static BUNDLED: [Bundled; 11] = [
    bundled!("DejaVuSans.ttf", &[Script::Latin, Script::Greek, Script::Cyrillic, Script::Hebrew, Script::Armenian, Script::Georgian, Script::Common]),
    bundled!("NotoSansArabic-Regular.ttf", &[Script::Arabic]),
    bundled!("NotoSansBengali-Regular.ttf", &[Script::Bengali]),
    bundled!("NotoSansDevanagari-Regular.ttf", &[Script::Devanagari]),
    bundled!("NotoSansGujarati-Regular.ttf", &[Script::Gujarati]),
    bundled!("NotoSansGurmukhi-Regular.ttf", &[Script::Gurmukhi]),
    bundled!("NotoSansKannada-Regular.ttf", &[Script::Kannada]),
    bundled!("NotoSansMalayalam-Regular.ttf", &[Script::Malayalam]),
    bundled!("NotoSansOriya-Regular.ttf", &[Script::Oriya]),
    bundled!("NotoSansTamil-Regular.ttf", &[Script::Tamil]),
    bundled!("NotoSansTelugu-Regular.ttf", &[Script::Telugu]),
];

/// The compressed size of the bundled fonts, and inflated (tests, docs).
#[cfg(test)]
pub(crate) fn bundled_sizes() -> (usize, usize) {
    let z: usize = BUNDLED.iter().map(|b| b.data.len()).sum();
    let raw: usize = (0..BUNDLED.len()).filter_map(bundled).map(|id| get(id).data.len()).sum();
    (z, raw)
}

fn bundled(i: usize) -> Option<FontId> {
    static IDS: [OnceLock<Option<FontId>>; 11] = [const { OnceLock::new() }; 11];
    *IDS[i].get_or_init(|| {
        use std::io::Read as _;
        let mut raw = Vec::new();
        flate2::read::ZlibDecoder::new(BUNDLED[i].data).read_to_end(&mut raw).ok()?;
        load(Box::leak(raw.into_boxed_slice()), 0, None)
    })
}

/// The default font, DejaVu Sans.
pub(crate) fn default_font() -> &'static Font {
    get(bundled(0).expect("the bundled DejaVu Sans loads"))
}

/// The font for a character of a script (see the module docs for the
/// order); `None` when no font has it. `cjk` says which CJK fonts to try
/// first for Han characters (kana or Hangul elsewhere on the line).
pub(crate) fn font_for(c: char, script: Script, cjk: Cjk) -> Option<FontId> {
    let covers = |id: Option<FontId>| id.filter(|&id| get(id).has(c));
    if let Some(i) = BUNDLED.iter().position(|b| b.scripts.contains(&script) && script != Script::Common)
        && let Some(id) = covers(bundled(i))
    {
        return Some(id);
    }
    if let Some(id) = covers(bundled(0)) {
        return Some(id);
    }
    if let Some(id) = (1..BUNDLED.len()).find_map(|i| covers(bundled(i))) {
        return Some(id);
    }
    system::font_for(c, script, cjk)
}

/// Which CJK fonts to try first for Han.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Cjk {
    #[default]
    Chinese,
    Japanese,
    Korean,
}

/// Fonts in `dir` (and its subfolders) fill in characters the bundled
/// fonts lack, before the system's fonts: a CJK font on a machine without
/// one, or a font pack the user installed. Fonts already chosen stay.
pub fn add_font_dir(dir: impl AsRef<Path>) {
    system::add_dir(dir.as_ref().to_path_buf());
}

/// Whether the system's fonts may fill in characters the bundled fonts and
/// the added folders lack (on by default). Off, such characters print as
/// "?", and the PDF is the same on every machine.
pub fn set_system_fonts(on: bool) {
    system::set_enabled(on);
}

// ------------------------------------------------------------ system fonts

/// System fallback fonts, found by family name with `fontdb` the first time
/// a character needs one (reading the font folders takes a moment).
pub(crate) mod system {
    use super::*;

    /// Families to try for Chinese (Simplified first), Japanese and Korean,
    /// across Windows, macOS, iOS, Android and Linux.
    const CHINESE: &[&str] = &[
        "PingFang SC",
        "Hiragino Sans GB",
        "Heiti SC",
        "STHeiti",
        "Microsoft YaHei",
        "Microsoft YaHei UI",
        "DengXian",
        "SimHei",
        "SimSun",
        "Noto Sans CJK SC",
        "Noto Sans SC",
        "Source Han Sans SC",
        "Source Han Sans CN",
        "WenQuanYi Micro Hei",
        "WenQuanYi Zen Hei",
        "Droid Sans Fallback",
        "Microsoft JhengHei",
        "PingFang TC",
        "Noto Sans CJK TC",
        "Songti SC",
    ];
    const JAPANESE: &[&str] = &[
        "Hiragino Sans",
        "Hiragino Kaku Gothic ProN",
        "Hiragino Kaku Gothic Pro",
        "Yu Gothic",
        "Yu Gothic UI",
        "Meiryo",
        "MS Gothic",
        "Noto Sans CJK JP",
        "Noto Sans JP",
        "Source Han Sans JP",
        "IPAexGothic",
        "IPAGothic",
        "TakaoGothic",
        "VL Gothic",
    ];
    const KOREAN: &[&str] = &[
        "Apple SD Gothic Neo",
        "AppleGothic",
        "Malgun Gothic",
        "Noto Sans CJK KR",
        "Noto Sans KR",
        "Source Han Sans KR",
        "NanumGothic",
        "UnDotum",
        "Gulim",
    ];
    /// Broad fonts tried last for any script.
    const ANY: &[&str] = &[
        "Nirmala UI",
        "Segoe UI",
        "Segoe UI Historic",
        "Segoe UI Symbol",
        "Arial Unicode MS",
        "Noto Sans",
        "Noto Sans Symbols",
        "Noto Sans Symbols 2",
        "Apple Symbols",
        "FreeSans",
        "Droid Sans Fallback",
    ];

    struct System {
        enabled: bool,
        /// Folders added with `add_font_dir`, and how many are loaded.
        dirs: Vec<PathBuf>,
        loaded_dirs: usize,
        added: Vec<FontId>,
        /// The system's fonts, read on first need.
        db: Option<fontdb::Database>,
        /// Fonts loaded, by fontdb face.
        faces: HashMap<fontdb::ID, Option<FontId>>,
        /// The font of each family tried (by name).
        families: HashMap<String, Option<FontId>>,
        /// System families named after each script.
        named: HashMap<Script, Vec<String>>,
    }

    fn state() -> &'static Mutex<System> {
        static S: OnceLock<Mutex<System>> = OnceLock::new();
        S.get_or_init(|| {
            Mutex::new(System { enabled: true, dirs: Vec::new(), loaded_dirs: 0, added: Vec::new(), db: None, faces: HashMap::new(), families: HashMap::new(), named: HashMap::new() })
        })
    }

    pub(super) fn add_dir(dir: PathBuf) {
        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
        if !s.dirs.contains(&dir) {
            s.dirs.push(dir);
        }
    }

    pub(super) fn set_enabled(on: bool) {
        state().lock().unwrap_or_else(|e| e.into_inner()).enabled = on;
    }

    /// Load a face of a database as a font (once).
    fn face_font(db: &mut fontdb::Database, faces: &mut HashMap<fontdb::ID, Option<FontId>>, id: fontdb::ID) -> Option<FontId> {
        if let Some(&f) = faces.get(&id) {
            return f;
        }
        let emoji = db.face(id).is_some_and(|f| f.families.iter().any(|(n, _)| n.contains("Emoji")));
        let ps_name = db.face(id).map(|f| f.post_script_name.clone());
        let f = if emoji {
            None
        } else {
            let face = match db.face_source(id) {
                Some((fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _), index)) => read_face(&path, index),
                Some((fontdb::Source::Binary(data), index)) => Some(((*data).as_ref().to_vec(), index)),
                None => None,
            };
            // Fonts stay loaded for the rest of the run.
            face.and_then(|(data, index)| load(Box::leak(data.into_boxed_slice()), index, ps_name.as_deref()))
        };
        faces.insert(id, f);
        f
    }

    /// One face of a font file: the whole file, or from a collection
    /// (`.ttc`, often tens of megabytes of weights) only that face's tables.
    fn read_face(path: &Path, index: u32) -> Option<(Vec<u8>, u32)> {
        use std::io::{Read as _, Seek as _, SeekFrom};
        let mut f = std::fs::File::open(path).ok()?;
        let mut read_at = |at: u64, len: usize| -> Option<Vec<u8>> {
            let mut b = vec![0; len];
            f.seek(SeekFrom::Start(at)).ok()?;
            f.read_exact(&mut b).ok()?;
            Some(b)
        };
        let head = read_at(0, 12)?;
        if &head[..4] != b"ttcf" {
            return Some((std::fs::read(path).ok()?, index));
        }
        let be32 = |b: &[u8], i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        if index >= be32(&head, 8) {
            return None;
        }
        let face_at = u64::from(be32(&read_at(12 + 4 * u64::from(index), 4)?, 0));
        let sfnt = read_at(face_at, 12)?;
        let tables = usize::from(u16::from_be_bytes([sfnt[4], sfnt[5]]));
        let dir = read_at(face_at + 12, 16 * tables)?;
        let mut out = sfnt.clone();
        let mut data = Vec::new();
        let start = 12 + 16 * tables;
        for t in 0..tables {
            let r = &dir[16 * t..16 * t + 16];
            let (off, len) = (be32(r, 8), be32(r, 12) as usize);
            let at = start + data.len();
            out.extend_from_slice(&r[..8]);
            out.extend_from_slice(&u32::try_from(at).ok()?.to_be_bytes());
            out.extend_from_slice(&r[12..16]);
            data.extend(read_at(u64::from(off), len)?);
            data.resize(data.len().div_ceil(4) * 4, 0);
        }
        out.extend(data);
        Some((out, 0))
    }

    fn system_db() -> fontdb::Database {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        // fontdb reads no folder on Android; these are the system's.
        #[cfg(target_os = "android")]
        for dir in ["/system/fonts", "/product/fonts", "/system/product/fonts"] {
            db.load_fonts_dir(dir);
        }
        db
    }

    /// The families to try for a script, best first.
    fn families(script: Script, cjk: Cjk) -> Vec<&'static str> {
        let han = matches!(script, Script::Han | Script::Bopomofo);
        let kana = matches!(script, Script::Hiragana | Script::Katakana);
        let lists: [&[&'static str]; 3] = if kana || (han && cjk == Cjk::Japanese) {
            [JAPANESE, CHINESE, KOREAN]
        } else if script == Script::Hangul || (han && cjk == Cjk::Korean) {
            [KOREAN, CHINESE, JAPANESE]
        } else if han {
            [CHINESE, JAPANESE, KOREAN]
        } else {
            [&[], &[], &[]]
        };
        lists.into_iter().flatten().chain(ANY).copied().collect()
    }

    /// System families named after a script ("Noto Sans Thai", "Sinhala
    /// Sangam MN", "Myanmar Text"), sans-serif ones first.
    fn named_after(db: &fontdb::Database, script: Script) -> Vec<String> {
        if matches!(script, Script::Common | Script::Inherited | Script::Unknown | Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul | Script::Bopomofo) {
            return Vec::new();
        }
        let name = script.full_name().replace('_', " ").to_ascii_lowercase();
        let mut named: Vec<String> = db
            .faces()
            .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
            .filter(|n| {
                let n = n.to_ascii_lowercase();
                n.match_indices(&name).any(|(i, _)| {
                    let before = n[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
                    let after = n[i + name.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric());
                    before && after
                })
            })
            .collect();
        named.sort();
        named.dedup();
        named.sort_by_key(|n| !n.contains("Sans"));
        named
    }

    pub(super) fn font_for(c: char, script: Script, cjk: Cjk) -> Option<FontId> {
        let mut guard = state().lock().unwrap_or_else(|e| e.into_inner());
        let s = &mut *guard;
        // Added folders first: every face in them, in order.
        if s.loaded_dirs < s.dirs.len() {
            let mut db = fontdb::Database::new();
            for d in &s.dirs[s.loaded_dirs..] {
                db.load_fonts_dir(d);
            }
            s.loaded_dirs = s.dirs.len();
            let mut faces = HashMap::new();
            let ids: Vec<fontdb::ID> = db.faces().map(|f| f.id).collect();
            for id in ids {
                if let Some(f) = face_font(&mut db, &mut faces, id) {
                    s.added.push(f);
                }
            }
        }
        if let Some(&f) = s.added.iter().find(|&&f| get(f).has(c)) {
            return Some(f);
        }
        if !s.enabled || script == Script::Unknown {
            return None;
        }
        let db = s.db.get_or_insert_with(system_db);
        let named = s.named.entry(script).or_insert_with(|| named_after(db, script));
        let candidates: Vec<String> = named.iter().cloned().chain(families(script, cjk).into_iter().map(str::to_owned)).collect();
        for family in candidates {
            let f = match s.families.get(&family) {
                Some(&f) => f,
                None => {
                    let query = fontdb::Query { families: &[fontdb::Family::Name(&family)], ..fontdb::Query::default() };
                    let f = db.query(&query).and_then(|id| face_font(db, &mut s.faces, id));
                    s.families.insert(family, f);
                    f
                }
            };
            if let Some(f) = f.filter(|&f| get(f).has(c)) {
                return Some(f);
            }
        }
        None
    }

    /// Whether any CJK font can be found here (tests).
    #[cfg(test)]
    pub(crate) fn has_cjk() -> bool {
        font_for('\u{4E2D}', Script::Han, Cjk::Chinese).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_fonts_cover_their_scripts() {
        let f = default_font();
        assert!(f.upem > 0 && f.ascent > 0 && f.descent < 0 && !f.cff);
        for c in ['A', 'z', '0', 'é', 'ñ', 'ü', '£', '€', '¥', 'ß', 'Ø', '₹', 'Ω', 'Ж', 'א'] {
            assert_eq!(font_for(c, Script::Latin, Cjk::Chinese), Some(f.id), "{c:?} in DejaVu Sans");
        }
        assert!(f.advance_1000(f.glyph('M').unwrap()) > 500);
        for (c, script, name) in [
            ('क', Script::Devanagari, "NotoSansDevanagari-Regular"),
            ('ળ', Script::Gujarati, "NotoSansGujarati-Regular"),
            ('த', Script::Tamil, "NotoSansTamil-Regular"),
            ('త', Script::Telugu, "NotoSansTelugu-Regular"),
            ('ಕ', Script::Kannada, "NotoSansKannada-Regular"),
            ('മ', Script::Malayalam, "NotoSansMalayalam-Regular"),
            ('ক', Script::Bengali, "NotoSansBengali-Regular"),
            ('ਪ', Script::Gurmukhi, "NotoSansGurmukhi-Regular"),
            ('ଓ', Script::Oriya, "NotoSansOriya-Regular"),
            ('ب', Script::Arabic, "NotoSansArabic-Regular"),
        ] {
            let id = font_for(c, script, Cjk::Chinese).unwrap_or_else(|| panic!("{c}"));
            assert_eq!(get(id).name, name);
        }
        // Compressed in the binary to well under the fonts' size.
        let (z, raw) = bundled_sizes();
        assert!(z * 10 < raw * 7, "{z} of {raw}");
    }

    #[test]
    fn unknown_characters_have_no_font() {
        assert_eq!(font_for('\u{0378}', Script::Unknown, Cjk::Chinese), None, "unassigned");
    }
}

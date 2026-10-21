//! Checking the logo and signature a company adds to its invoices.
//!
//! PNG and JPEG files are checked by their headers (and a PNG is decoded
//! once, so a broken file is refused now rather than at print time). SVG
//! files are rebuilt from a list of allowed elements and attributes: no
//! scripts, event handlers, external links, embedded HTML or entity
//! declarations survive. Anything that can't be rebuilt is refused. The
//! images are only ever shown through `<img>` (where SVGs run no script
//! and load nothing), so this is a second line of defence.

use crate::design::{Image, ImageKind};

/// Largest logo or signature file accepted.
pub const MAX_IMAGE_BYTES: usize = 300 * 1024;
/// Largest width or height in pixels of a PNG or JPEG.
pub const MAX_PIXELS: u32 = 4000;

/// Check an image file and return it ready to store (an SVG sanitised).
pub fn check_image(bytes: &[u8]) -> Result<Image, String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!("the image is {} KB; use one of {} KB or less", bytes.len().div_ceil(1024), MAX_IMAGE_BYTES / 1024));
    }
    if bytes.is_empty() {
        return Err("the file is empty".into());
    }
    let image = |kind, data: Vec<u8>| Image { kind, data, raster: None };
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let (w, h) = png_size(bytes)?;
        check_pixels(w, h)?;
        return Ok(image(ImageKind::Png, bytes.to_vec()));
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        let (w, h, _) = jpeg_info(bytes).ok_or("this JPEG can't be read: save it again as a baseline JPEG or as PNG")?;
        check_pixels(w.into(), h.into())?;
        return Ok(image(ImageKind::Jpeg, bytes.to_vec()));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "use a PNG, JPEG or SVG image".to_owned())?;
    if text.contains("<svg") {
        return Ok(image(ImageKind::Svg, sanitize_svg(text)?.into_bytes()));
    }
    Err("use a PNG, JPEG or SVG image".into())
}

/// Check a PNG rendition of an SVG made by the app's webview.
pub fn check_raster(bytes: &[u8]) -> Result<(), String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("the rendition must be a PNG".into());
    }
    if bytes.len() > 4 * MAX_IMAGE_BYTES {
        return Err("the rendition is too large".into());
    }
    let (w, h) = png_size(bytes)?;
    check_pixels(w, h)
}

fn check_pixels(w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 {
        return Err("the image has no pixels".into());
    }
    if w > MAX_PIXELS || h > MAX_PIXELS {
        return Err(format!("the image is {w} × {h} pixels; use one at most {MAX_PIXELS} pixels wide and high"));
    }
    Ok(())
}

fn png_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    let bad = |e: png::DecodingError| format!("this PNG can't be read ({e})");
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().map_err(bad)?;
    let (w, h) = (reader.info().width, reader.info().height);
    check_pixels(w, h)?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("this PNG is too large")?];
    reader.next_frame(&mut buf).map_err(bad)?;
    Ok((w, h))
}

/// Width, height and components of a JPEG, from its first SOF0–SOF2 frame.
pub fn jpeg_info(b: &[u8]) -> Option<(u16, u16, u8)> {
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if matches!(marker, 0xD8 | 0x01 | 0xD0..=0xD7) {
            i += 2;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
        if matches!(marker, 0xC0..=0xC2) {
            let s = b.get(i + 4..i + 10)?;
            let (h, w, c) = (u16::from_be_bytes([s[1], s[2]]), u16::from_be_bytes([s[3], s[4]]), s[5]);
            return (w > 0 && h > 0 && matches!(c, 1 | 3 | 4)).then_some((w, h, c));
        }
        if matches!(marker, 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF | 0xD9 | 0xDA) || len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

// ------------------------------------------------------------ SVG

/// Elements kept as they are (with their allowed attributes).
const ELEMENTS: &[&str] = &[
    "svg", "g", "path", "rect", "circle", "ellipse", "line", "polyline", "polygon", "defs", "linearGradient", "radialGradient",
    "stop", "clipPath", "mask", "pattern", "symbol", "use", "text", "tspan", "title", "desc", "style",
];

/// Editor and metadata elements dropped with everything inside them.
const DROPPED: &[&str] = &["metadata", "sodipodi:namedview", "inkscape:perspective", "inkscape:path-effect"];

const ATTRIBUTES: &[&str] = &[
    "id", "class", "d", "x", "y", "x1", "y1", "x2", "y2", "cx", "cy", "r", "rx", "ry", "fx", "fy", "width", "height", "viewBox",
    "points", "fill", "fill-opacity", "fill-rule", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin",
    "stroke-miterlimit", "stroke-dasharray", "stroke-dashoffset", "stroke-opacity", "opacity", "transform", "offset",
    "stop-color", "stop-opacity", "gradientUnits", "gradientTransform", "spreadMethod", "clip-path", "clip-rule",
    "clipPathUnits", "mask", "maskUnits", "maskContentUnits", "patternUnits", "patternContentUnits", "patternTransform", "xmlns",
    "xmlns:xlink", "version", "preserveAspectRatio", "font-family", "font-size", "font-weight", "font-style", "text-anchor",
    "dominant-baseline", "letter-spacing", "dx", "dy", "href", "xlink:href", "visibility", "display", "color", "xml:space",
    "vector-effect", "shape-rendering", "paint-order", "style",
];

/// Rebuild an SVG from allowed parts, or say why it can't be used.
pub fn sanitize_svg(src: &str) -> Result<String, String> {
    let refuse = |what: &str| Err(format!("this SVG can't be used: {what}. Save it as a plain SVG or a PNG"));
    let mut out = String::with_capacity(src.len());
    let mut stack: Vec<String> = Vec::new();
    // Depth of a dropped element we are inside (its content is skipped).
    let mut skipping: Option<usize> = None;
    let mut seen_svg = false;
    let mut rest = src.trim_start_matches('\u{feff}');
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            if !rest.trim().is_empty() && skipping.is_none() {
                if stack.is_empty() {
                    return refuse("text outside the drawing");
                }
                out.push_str(&text(rest)?);
            }
            break;
        };
        let (txt, tag) = rest.split_at(lt);
        if skipping.is_none() && !txt.trim().is_empty() {
            if stack.is_empty() {
                return refuse("text outside the drawing");
            }
            out.push_str(&text(txt)?);
        } else if skipping.is_none() && !stack.is_empty() {
            out.push_str(txt);
        }
        if let Some(after) = tag.strip_prefix("<!--") {
            let end = after.find("-->").ok_or("an unfinished comment")?;
            rest = &after[end + 3..];
            continue;
        }
        if tag.starts_with("<?") {
            let end = tag.find("?>").ok_or("an unfinished declaration")?;
            rest = &tag[end + 2..];
            continue;
        }
        if tag.starts_with("<!DOCTYPE") || tag.starts_with("<!doctype") || tag.starts_with("<!ENTITY") {
            return refuse("it declares a document type or entities");
        }
        if let Some(after) = tag.strip_prefix("<![CDATA[") {
            let end = after.find("]]>").ok_or("an unfinished CDATA section")?;
            if stack.last().map(String::as_str) != Some("style") {
                return refuse("character data outside a style sheet");
            }
            if skipping.is_none() {
                out.push_str(&style_text(&after[..end])?);
            }
            rest = &after[end + 3..];
            continue;
        }
        let close = tag.find('>').ok_or("an unfinished tag")?;
        let inner = &tag[1..close];
        rest = &tag[close + 1..];
        if let Some(name) = inner.strip_prefix('/') {
            let name = name.trim();
            match stack.pop() {
                Some(open) if open == name => {}
                _ => return refuse("its tags don't match"),
            }
            if skipping == Some(stack.len()) {
                skipping = None;
            } else if skipping.is_none() {
                out.push_str(&format!("</{name}>"));
            }
            continue;
        }
        let self_closing = inner.ends_with('/');
        let inner = inner.trim_end_matches('/');
        let (name, attrs) = match inner.find(|c: char| c.is_whitespace()) {
            Some(i) => (&inner[..i], &inner[i..]),
            None => (inner, ""),
        };
        if name.is_empty() {
            return refuse("an empty tag");
        }
        if skipping.is_some() {
            if !self_closing {
                stack.push(name.to_owned());
            }
            continue;
        }
        if DROPPED.contains(&name) || name.starts_with("sodipodi:") || name.starts_with("inkscape:") {
            if !self_closing {
                skipping = Some(stack.len());
                stack.push(name.to_owned());
            }
            continue;
        }
        if !ELEMENTS.contains(&name) {
            return refuse(&format!("it contains <{name}>"));
        }
        if stack.is_empty() {
            if name != "svg" || seen_svg {
                return refuse("it isn't a single <svg> drawing");
            }
            seen_svg = true;
        }
        out.push('<');
        out.push_str(name);
        for (k, v) in attributes(attrs).ok_or("its attributes can't be read")? {
            let k_lower = k.to_ascii_lowercase();
            if k_lower.starts_with("on") || !ATTRIBUTES.contains(&k) {
                continue; // event handlers and editor attributes
            }
            let v_lower = v.to_ascii_lowercase();
            if k == "href" || k == "xlink:href" {
                if !v.starts_with('#') {
                    return refuse("it links to another file or address");
                }
            } else if v_lower.contains("url(") && !only_local_urls(&v_lower) {
                return refuse("it refers to another file or address");
            }
            if v_lower.contains("javascript:") || v_lower.contains("expression(") {
                return refuse("it contains script");
            }
            if k == "style" {
                style_text(&v)?;
            }
            out.push_str(&format!(" {k}=\"{}\"", attr_value(&v)?));
        }
        if self_closing {
            out.push_str("/>");
        } else {
            out.push('>');
            stack.push(name.to_owned());
        }
        if name == "style" && !self_closing {
            // A style sheet's text: check it as CSS, not as markup.
            let end = rest.find("</style").ok_or("an unfinished style sheet")?;
            let css = &rest[..end];
            if let Some(cdata) = css.trim().strip_prefix("<![CDATA[") {
                out.push_str(&style_text(cdata.trim_end().trim_end_matches("]]>"))?);
            } else {
                if css.contains('<') {
                    return refuse("markup inside a style sheet");
                }
                out.push_str(&style_text(css)?);
            }
            rest = &rest[end..];
        }
    }
    if !stack.is_empty() || !seen_svg {
        return refuse("it isn't a complete <svg> drawing");
    }
    Ok(out)
}

/// `url(#id)` only.
fn only_local_urls(v: &str) -> bool {
    v.match_indices("url(").all(|(i, _)| v[i + 4..].trim_start().trim_start_matches(['"', '\'']).starts_with('#'))
}

fn style_text(css: &str) -> Result<String, String> {
    let l = css.to_ascii_lowercase();
    if l.contains("@import") || l.contains("javascript:") || l.contains("expression(") || l.contains("-moz-binding") || l.contains("behavior:") {
        return Err("this SVG can't be used: its style sheet loads or runs something. Save it as a plain SVG or a PNG".into());
    }
    if l.contains("url(") && !only_local_urls(&l) {
        return Err("this SVG can't be used: its style sheet refers to another file or address. Save it as a plain SVG or a PNG".into());
    }
    Ok(css.replace('<', "&lt;"))
}

/// Text content, with only the predefined and numeric entities.
fn text(s: &str) -> Result<String, String> {
    check_entities(s)?;
    Ok(s.replace('>', "&gt;"))
}

fn attr_value(v: &str) -> Result<String, String> {
    check_entities(v)?;
    Ok(v.replace('"', "&quot;").replace('<', "&lt;"))
}

fn check_entities(s: &str) -> Result<(), String> {
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        let after = &rest[i + 1..];
        let end = after.find(';').filter(|&e| e <= 10).ok_or("this SVG can't be used: a stray &")?;
        let name = &after[..end];
        let ok = matches!(name, "amp" | "lt" | "gt" | "quot" | "apos")
            || name.strip_prefix("#x").is_some_and(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()))
            || name.strip_prefix('#').is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()));
        if !ok {
            return Err(format!("this SVG can't be used: it uses the entity &{name};"));
        }
        rest = &after[end + 1..];
    }
    Ok(())
}

/// `name="value"` pairs (either quote).
fn attributes(s: &str) -> Option<Vec<(&str, String)>> {
    let mut out = Vec::new();
    let mut rest = s.trim_start();
    while !rest.is_empty() {
        let eq = rest.find('=')?;
        let name = rest[..eq].trim();
        if name.is_empty() || name.contains(char::is_whitespace) {
            return None;
        }
        let after = rest[eq + 1..].trim_start();
        let q = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let end = after[1..].find(q)?;
        out.push((name, after[1..1 + end].to_owned()));
        rest = after[end + 2..].trim_start();
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, w, h);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            let mut wr = e.write_header().unwrap();
            wr.write_image_data(&vec![200; (w * h * 4) as usize]).unwrap();
        }
        out
    }

    #[test]
    fn pngs_are_checked_and_size_limited() {
        let ok = check_image(&png(40, 20)).unwrap();
        assert_eq!(ok.kind, ImageKind::Png);
        assert!(ok.data_url().starts_with("data:image/png;base64,iVBORw0KGgo"));
        let mut broken = png(40, 20);
        broken.truncate(broken.len() - 20);
        assert!(check_image(&broken).is_err());
        assert!(check_image(&vec![0u8; MAX_IMAGE_BYTES + 1]).unwrap_err().contains("KB"));
        assert!(check_image(b"GIF89a....").is_err());
        assert!(check_raster(&png(4, 4)).is_ok());
        assert!(check_raster(b"<svg/>").is_err());
    }

    #[test]
    fn jpeg_frames_are_read() {
        // SOI, an APP0 segment, then SOF0: 8-bit, 30 high, 50 wide, 3 components.
        let mut j = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46];
        j.extend([0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x1E, 0x00, 0x32, 0x03]);
        j.extend([0; 9]);
        assert_eq!(jpeg_info(&j), Some((50, 30, 3)));
        assert_eq!(check_image(&j).unwrap().kind, ImageKind::Jpeg);
        assert_eq!(jpeg_info(&[0xFF, 0xD8, 0xFF, 0xC3, 0, 4, 0, 0]), None, "lossless JPEG is not drawn");
    }

    #[test]
    fn plain_svgs_are_kept() {
        let src = r##"<?xml version="1.0"?><!-- logo --><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" inkscape:version="1"><metadata><rdf:RDF/></metadata><defs><linearGradient id="g"><stop offset="0" stop-color="#f00"/></linearGradient></defs><g fill="url(#g)"><rect width="10" height="10" onclick="alert(1)"/><text x="1" y="8">A &amp; B</text></g><style>.a{fill:#123}</style></svg>"##;
        let out = sanitize_svg(src).unwrap();
        assert!(out.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\">"), "{out}");
        assert!(out.contains("fill=\"url(#g)\"") && out.contains("A &amp; B") && out.contains(".a{fill:#123}"));
        assert!(!out.contains("onclick") && !out.contains("metadata") && !out.contains("inkscape") && !out.contains("<!--"));
        assert_eq!(check_image(src.as_bytes()).unwrap().kind, ImageKind::Svg);
    }

    #[test]
    fn dangerous_svgs_are_refused() {
        for bad in [
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<svg><foreignObject><div/></foreignObject></svg>"#,
            r#"<svg><image href="http://x/a.png"/></svg>"#,
            r#"<svg><use href="http://x/a.svg#a"/></svg>"#,
            r#"<svg><use xlink:href="data:image/svg+xml,AAA"/></svg>"#,
            r#"<svg><rect fill="url(http://x/a)"/></svg>"#,
            r#"<!DOCTYPE svg [<!ENTITY x "y">]><svg>&x;</svg>"#,
            r#"<svg><style>@import url(http://x/a.css);</style></svg>"#,
            r#"<svg><a href="javascript:alert(1)"><rect/></a></svg>"#,
            r#"<svg><animate attributeName="href" to="javascript:alert(1)"/></svg>"#,
            r#"<svg><g></svg>"#,
            r#"<svg/><svg/>"#,
            r#"<html><svg/></html>"#,
        ] {
            assert!(sanitize_svg(bad).is_err(), "{bad}");
        }
    }
}

//! Logos and signatures as PDF image XObjects.
//!
//! JPEG files go in as they are (`/DCTDecode`); only the frame header is
//! read for the size and colour space. PNG files are decoded with the `png`
//! crate (palette and low bit depths expanded, 16-bit cut to 8) and written
//! as compressed RGB or grey samples, with any alpha channel as a soft
//! mask. An image that can't be read is left out of the PDF: the app
//! checked it when it was added.

use crate::ImageKind;

/// An image ready to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PdfImage {
    pub width: u32,
    pub height: u32,
    /// `/DeviceRGB`, `/DeviceGray` or `/DeviceCMYK`.
    pub colour_space: &'static str,
    /// `/DCTDecode` (JPEG as is) or `/FlateDecode` (raw samples, compressed by the writer).
    pub filter: &'static str,
    pub data: Vec<u8>,
    /// Alpha samples (8-bit), for an `/SMask`.
    pub alpha: Option<Vec<u8>>,
    /// Adobe CMYK JPEGs store inverted values.
    pub invert_cmyk: bool,
}

/// Decode an image for the PDF, or `None` when it can't be read.
pub(crate) fn load(kind: ImageKind, bytes: &[u8]) -> Option<PdfImage> {
    match kind {
        ImageKind::Jpeg => jpeg(bytes),
        ImageKind::Png => png_image(bytes),
        ImageKind::Svg => None,
    }
}

/// Size, components and Adobe marker of a baseline, extended or progressive JPEG.
pub(crate) fn jpeg_info(b: &[u8]) -> Option<(u32, u32, u8, bool)> {
    if b.get(0..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    let mut adobe = false;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        let seg = b.get(i + 4..i + 2 + len)?;
        match marker {
            0xEE if seg.starts_with(b"Adobe") => adobe = true,
            0xC0..=0xC2 => {
                let h = u16::from_be_bytes([*seg.get(1)?, *seg.get(2)?]) as u32;
                let w = u16::from_be_bytes([*seg.get(3)?, *seg.get(4)?]) as u32;
                let n = *seg.get(5)?;
                return (w > 0 && h > 0).then_some((w, h, n, adobe));
            }
            0xDA | 0xD9 => return None,
            _ => {}
        }
        i += 2 + len;
    }
    None
}

fn jpeg(b: &[u8]) -> Option<PdfImage> {
    let (width, height, n, adobe) = jpeg_info(b)?;
    let colour_space = match n {
        1 => "/DeviceGray",
        3 => "/DeviceRGB",
        4 => "/DeviceCMYK",
        _ => return None,
    };
    Some(PdfImage { width, height, colour_space, filter: "/DCTDecode", data: b.to_vec(), alpha: None, invert_cmyk: n == 4 && adobe })
}

fn png_image(b: &[u8]) -> Option<PdfImage> {
    let mut d = png::Decoder::new(std::io::Cursor::new(b));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let (w, h) = (info.width, info.height);
    let px = (w as usize) * (h as usize);
    let samples = &buf[..info.line_size * h as usize];
    // Rows are packed (no padding at 8 bits), so samples go straight through.
    let (colour_space, data, alpha) = match info.color_type {
        png::ColorType::Rgb => ("/DeviceRGB", samples.to_vec(), None),
        png::ColorType::Grayscale => ("/DeviceGray", samples.to_vec(), None),
        png::ColorType::Rgba => {
            let mut rgb = Vec::with_capacity(px * 3);
            let mut a = Vec::with_capacity(px);
            for p in samples.chunks_exact(4) {
                rgb.extend_from_slice(&p[..3]);
                a.push(p[3]);
            }
            ("/DeviceRGB", rgb, Some(a))
        }
        png::ColorType::GrayscaleAlpha => {
            let mut g = Vec::with_capacity(px);
            let mut a = Vec::with_capacity(px);
            for p in samples.chunks_exact(2) {
                g.push(p[0]);
                a.push(p[1]);
            }
            ("/DeviceGray", g, Some(a))
        }
        png::ColorType::Indexed => return None, // EXPAND turns palettes into RGB(A)
    };
    // A fully opaque mask is no mask.
    let alpha = alpha.filter(|a| a.iter().any(|&v| v != 255));
    Some(PdfImage { width: w, height: h, colour_space, filter: "/FlateDecode", data, alpha, invert_cmyk: false })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A PNG made with the png crate's encoder.
    pub(crate) fn png_bytes(w: u32, h: u32, colour: png::ColorType, px: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, w, h);
            e.set_color(colour);
            e.set_depth(png::BitDepth::Eight);
            let mut wr = e.write_header().unwrap();
            wr.write_image_data(px).unwrap();
        }
        out
    }

    /// The start of a baseline JPEG: SOI, APP14 "Adobe", SOF0 (h × w, n components).
    pub(crate) fn jpeg_head(w: u16, h: u16, n: u8) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xEE, 0x00, 0x0E];
        b.extend_from_slice(b"Adobe");
        b.extend_from_slice(&[0, 100, 0, 0, 0, 0, 1]);
        b.extend_from_slice(&[0xFF, 0xC0]);
        let len = 8 + 3 * u16::from(n);
        b.extend_from_slice(&len.to_be_bytes());
        b.push(8);
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&w.to_be_bytes());
        b.push(n);
        for c in 0..n {
            b.extend_from_slice(&[c + 1, 0x11, 0]);
        }
        b.extend_from_slice(&[0xFF, 0xD9]);
        b
    }

    #[test]
    fn pngs_decode_with_their_alpha() {
        let rgba = png_bytes(2, 1, png::ColorType::Rgba, &[255, 0, 0, 255, 0, 0, 255, 0]);
        let i = load(ImageKind::Png, &rgba).unwrap();
        assert_eq!((i.width, i.height, i.colour_space, i.filter), (2, 1, "/DeviceRGB", "/FlateDecode"));
        assert_eq!(i.data, [255, 0, 0, 0, 0, 255]);
        assert_eq!(i.alpha.as_deref(), Some(&[255, 0][..]));
        let opaque = png_bytes(1, 1, png::ColorType::Rgba, &[1, 2, 3, 255]);
        assert_eq!(load(ImageKind::Png, &opaque).unwrap().alpha, None);
        let grey = png_bytes(1, 2, png::ColorType::Grayscale, &[0, 200]);
        assert_eq!(load(ImageKind::Png, &grey).unwrap().colour_space, "/DeviceGray");
        assert_eq!(load(ImageKind::Png, b"not a png"), None);
    }

    #[test]
    fn jpeg_frames_give_size_and_colour_space() {
        assert_eq!(jpeg_info(&jpeg_head(640, 480, 3)), Some((640, 480, 3, true)));
        let cmyk = load(ImageKind::Jpeg, &jpeg_head(10, 20, 4)).unwrap();
        assert_eq!((cmyk.width, cmyk.height, cmyk.colour_space, cmyk.invert_cmyk), (10, 20, "/DeviceCMYK", true));
        assert_eq!(load(ImageKind::Jpeg, &jpeg_head(10, 20, 1)).unwrap().colour_space, "/DeviceGray");
        assert_eq!(jpeg_info(b"\xFF\xD8\xFF\xD9"), None);
        assert_eq!(load(ImageKind::Svg, b"<svg/>"), None);
    }
}

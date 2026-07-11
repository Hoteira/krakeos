//! Subsets a TTF down to the printable-ASCII glyphs. The output is a normal
//! TTF — the OS still rasterizes it at runtime with titanf — it just parses
//! in milliseconds instead of ~16s, because titanf eagerly caches every
//! glyph outline and the full Nerd font has ~10k of them.
//!
//! The `subsetter` crate targets PDF embedding, so it drops the cmap table
//! and remaps glyph ids; we rebuild a format-4 cmap for the new ids here.
//! The result is self-tested with titanf before writing, so a broken subset
//! fails on the host at build time, never inside the OS.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use subsetter::GlyphRemapper;

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

/// Build a cmap table (format 4, platform 3 / encoding 1) for char -> gid.
fn build_cmap(map: &BTreeMap<u16, u16>) -> Vec<u8> {
    // Contiguous char runs where gid advances by 1 share a segment
    let mut segs: Vec<(u16, u16, u16)> = Vec::new(); // (start_char, end_char, start_gid)
    for (&c, &g) in map {
        match segs.last_mut() {
            Some((s, e, sg)) if c == *e + 1 && g == *sg + (c - *s) => *e = c,
            _ => segs.push((c, c, g)),
        }
    }
    segs.push((0xFFFF, 0xFFFF, 0)); // required terminator

    let seg_count = segs.len() as u16;
    let search_range = 2 * (1u16 << (15 - seg_count.leading_zeros() as u16));
    let entry_selector = 15 - seg_count.leading_zeros() as u16;
    let range_shift = 2 * seg_count - search_range;

    let sub_len = 16 + seg_count as usize * 8; // header(14) + pad(2) + 4 arrays
    let mut t = Vec::new();
    // cmap header: version 0, one encoding record (3,1) at offset 12
    t.extend_from_slice(&be16(0));
    t.extend_from_slice(&be16(1));
    t.extend_from_slice(&be16(3));
    t.extend_from_slice(&be16(1));
    t.extend_from_slice(&12u32.to_be_bytes());
    // format 4 subtable
    t.extend_from_slice(&be16(4));
    t.extend_from_slice(&be16(sub_len as u16));
    t.extend_from_slice(&be16(0)); // language
    t.extend_from_slice(&be16(seg_count * 2));
    t.extend_from_slice(&be16(search_range));
    t.extend_from_slice(&be16(entry_selector));
    t.extend_from_slice(&be16(range_shift));
    for &(_, e, _) in &segs {
        t.extend_from_slice(&be16(e));
    }
    t.extend_from_slice(&be16(0)); // reservedPad
    for &(s, _, _) in &segs {
        t.extend_from_slice(&be16(s));
    }
    for &(s, e, g) in &segs {
        // terminator maps 0xFFFF -> 0 via delta 1 (0xFFFF + 1 wraps to 0)
        let delta = if s == 0xFFFF && e == 0xFFFF { 1u16 } else { g.wrapping_sub(s) };
        t.extend_from_slice(&be16(delta));
    }
    for _ in &segs {
        t.extend_from_slice(&be16(0)); // idRangeOffset = 0 (delta mapping)
    }
    t
}

fn table_checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    for chunk in data.chunks(4) {
        let mut w = [0u8; 4];
        w[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(w));
    }
    sum
}

/// Insert a table into a TTF, rebuilding the directory (offsets shift).
fn insert_table(font: &[u8], tag: [u8; 4], table: &[u8]) -> Vec<u8> {
    let num = u16::from_be_bytes([font[4], font[5]]) as usize;
    let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::with_capacity(num + 1);
    for i in 0..num {
        let rec = 12 + i * 16;
        let t: [u8; 4] = font[rec..rec + 4].try_into().unwrap();
        let off = u32::from_be_bytes(font[rec + 8..rec + 12].try_into().unwrap()) as usize;
        let len = u32::from_be_bytes(font[rec + 12..rec + 16].try_into().unwrap()) as usize;
        tables.push((t, font[off..off + len].to_vec()));
    }
    tables.push((tag, table.to_vec()));
    tables.sort_by_key(|(t, _)| *t); // directory must be tag-sorted

    let n = tables.len() as u16;
    let mut out = Vec::new();
    out.extend_from_slice(&font[0..4]); // sfnt version
    out.extend_from_slice(&be16(n));
    let pow = n.ilog2() as u16;
    let search_range = 16 * (1u16 << pow);
    out.extend_from_slice(&be16(search_range));
    out.extend_from_slice(&be16(pow));
    out.extend_from_slice(&be16(n * 16 - search_range));

    let mut offset = 12 + tables.len() * 16;
    let mut blobs = Vec::new();
    for (t, data) in &tables {
        out.extend_from_slice(t);
        out.extend_from_slice(&table_checksum(data).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let padded = (data.len() + 3) & !3;
        offset += padded;
        blobs.push((data, padded));
    }
    for (data, padded) in blobs {
        out.extend_from_slice(data);
        out.resize(out.len() + (padded - data.len()), 0);
    }
    // Note: head.checkSumAdjustment is left stale; titanf ignores it.
    out
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: fontsubset <in.ttf> <out.ttf>");
        std::process::exit(1);
    }

    let data = fs::read(&args[1]).expect("failed to read font");

    // Map printable ASCII to old glyph ids via titanf's cmap, remap in char
    // order so ids stay dense.
    let src = titanf::TrueTypeFont::load_font(&data).expect("failed to parse source font");
    let mut remapper = GlyphRemapper::new(); // reserves gid 0 (.notdef)
    let mut char_to_new: BTreeMap<u16, u16> = BTreeMap::new();
    for c in 32u8..=126 {
        let old = src.lookup_glyph_index(c as char) as u16;
        if old != 0 {
            char_to_new.insert(c as u16, remapper.remap(old));
        }
    }

    let subset = subsetter::subset(&data, 0, &remapper).expect("subsetting failed");
    let cmap = build_cmap(&char_to_new);
    let out = insert_table(&subset, *b"cmap", &cmap);

    // Self-test with the same rasterizer the OS uses
    let mut test = titanf::TrueTypeFont::load_font(&out)
        .expect("SELF-TEST FAILED: titanf cannot parse the subset font");
    for probe in ['A', 'g', '0', '>'] {
        let (metrics, bmp) = test.get_char::<false>(probe, 16.0);
        assert!(
            metrics.width > 0 && !bmp.is_empty(),
            "SELF-TEST FAILED: glyph '{}' rasterized empty",
            probe
        );
    }

    fs::write(&args[2], &out).expect("failed to write subset font");
    println!(
        "fontsubset: {} ({} bytes) -> {} ({} bytes, {} chars)",
        args[1],
        data.len(),
        args[2],
        out.len(),
        char_to_new.len()
    );
}

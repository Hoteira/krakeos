//! Host-side PNG -> raw 0xAARRGGBB (u32 LE) converter, nearest-neighbor
//! scaled. The shell reads the .raw straight into its background buffer;
//! decoding the PNG under the wasmi interpreter instead takes ~2 minutes.

use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 5 {
        eprintln!("usage: png2raw <in.png> <out.raw> <width> <height>");
        std::process::exit(1);
    }
    let (w, h): (usize, usize) = (args[3].parse().unwrap(), args[4].parse().unwrap());

    let data = fs::read(&args[1]).expect("failed to read png");
    let decoder = png::Decoder::new(data.as_slice());
    let mut reader = decoder.read_info().expect("bad png");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("bad png frame");
    let (iw, ih) = (info.width as usize, info.height as usize);
    let bpp = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("unsupported color type {:?}", other),
    };

    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let sy = y * ih / h;
        for x in 0..w {
            let sx = x * iw / w;
            let i = (sy * iw + sx) * bpp;
            let px: u32 = 0xFF00_0000
                | ((buf[i] as u32) << 16)
                | ((buf[i + 1] as u32) << 8)
                | buf[i + 2] as u32;
            out.extend_from_slice(&px.to_le_bytes());
        }
    }

    fs::write(&args[2], &out).expect("failed to write raw");
    println!("png2raw: wrote {} ({} bytes, {}x{})", args[2], out.len(), w, h);
}

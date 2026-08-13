//! Print everything the binding knows about an image, in the spirit of
//! `iinfo -v`.
//!
//! ```text
//! cargo run --example info -- path/to/image.exr
//! ```

use oiio::{ImageInput, PixelFormat};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: info <image>");
        std::process::exit(2);
    };
    let path = std::path::PathBuf::from(path);

    let mut input = ImageInput::from_path(&path)?;
    let spec = input.image_spec()?;

    println!("{}", path.display());
    println!("  format:      {}", input.format_name());
    let [width, height, depth] = spec.dimensions();
    let origin = spec.origin();
    println!("  data window: {width} x {height} x {depth} at {origin:?}");
    let [full_width, full_height, full_depth] = spec.full_dimensions();
    println!(
        "  display:     {full_width} x {full_height} x {full_depth} at {:?}",
        spec.full_origin()
    );
    println!("  pixel type:  {}", spec.format());
    if spec.is_tiled() {
        let [tile_width, tile_height, tile_depth] = spec.tile_dimensions();
        println!("  tiles:       {tile_width} x {tile_height} x {tile_depth}");
    } else {
        println!("  tiles:       none, stored as scanlines");
    }
    println!("  deep:        {}", spec.is_deep());

    println!("  channels:    {}", spec.channel_count());
    for (index, name) in spec.channel_names().iter().enumerate() {
        let role = match Some(index as u32) {
            i if i == spec.alpha_channel() => " (alpha)",
            i if i == spec.z_channel() => " (depth)",
            _ => "",
        };
        println!("    {index}: {name}{role}");
    }

    // Walk the subimages and mip levels the file actually has.
    let mut subimage = 0;
    while let Ok(sub_spec) = input.image_spec_at(subimage, 0) {
        let mut levels = 0;
        while input.image_spec_at(subimage, levels).is_ok() {
            levels += 1;
            if levels > 32 {
                break;
            }
        }
        let [sub_width, sub_height, _] = sub_spec.dimensions();
        println!("  subimage {subimage}: {sub_width} x {sub_height}, {levels} mip level(s)");
        subimage += 1;
        if subimage > 32 {
            break;
        }
    }

    let attributes = spec.attributes();
    println!("  metadata:    {} attributes", attributes.len());
    for (name, value) in attributes {
        println!("    {name} = {value}");
    }

    // Deep files carry their pixels differently, so summarise them differently.
    if spec.is_deep() {
        let deep = input.read_deep_image()?;
        let mut samples = 0u64;
        let mut deepest = 0usize;
        for y in origin[1]..origin[1] + height as i32 {
            for x in origin[0]..origin[0] + width as i32 {
                let count = deep.sample_count(x, y)?;
                samples += count as u64;
                deepest = deepest.max(count);
            }
        }
        println!("  deep:        {samples} samples, deepest pixel holds {deepest}");
    } else if spec.format() != PixelFormat::Other {
        // Read the pixels to report the range actually present.
        let mut pixels = vec![0.0_f32; spec.element_count()?];
        input.read_image_into(&mut pixels)?;
        let (min, max) = pixels
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), &value| {
                (min.min(value), max.max(value))
            });
        println!("  value range: {min} to {max}");
    }

    input.close()?;
    Ok(())
}

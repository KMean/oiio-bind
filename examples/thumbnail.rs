//! Make a thumbnail: read an image, fit it into a box, convert it to a
//! display colour space, and write it out. A small but complete pipeline.
//!
//! ```text
//! cargo run --example thumbnail -- input.exr output.png 256
//! ```

use oiio::algo::{self, FitMode};
use oiio::{ColorConfig, ImageBuf, ImageSpec, PixelFormat};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, size] = arguments.as_slice() else {
        eprintln!("usage: thumbnail <input> <output> <size>");
        std::process::exit(2);
    };
    let size: u32 = size.parse()?;

    // Attaching does not read pixels, so the specification is available
    // before paying for them.
    let mut source = ImageBuf::from_path(std::path::Path::new(input))?;
    let spec = source.spec()?;
    let [width, height, _] = spec.dimensions();
    println!("{input}: {width} x {height}, {}", spec.format());
    source.read()?;

    // Fit into a square of the requested size, preserving aspect ratio. The
    // destination decides the frame, so it is allocated at that size.
    let frame = ImageSpec::new(size, size, spec.channel_count(), PixelFormat::F32)?;
    let mut fitted = ImageBuf::new(&frame)?;
    algo::fit(
        &mut fitted,
        &source,
        Some("lanczos3"),
        None,
        FitMode::Letterbox,
        false,
        None,
    )?;
    let [fitted_width, fitted_height, _] = fitted.spec()?.dimensions();
    println!("fitted to {fitted_width} x {fitted_height}");

    // Convert to something a screen expects, if the configuration offers it.
    // A thumbnail written straight from linear float looks far too dark.
    let config = ColorConfig::new()?;
    let display = config
        .color_space_for_role("color_picking")
        .or_else(|| config.color_space_for_role("texture_paint"));
    let converted = match display {
        Some(display) => {
            let linear = config
                .color_space_for_role("scene_linear")
                .unwrap_or_else(|| "linear".to_owned());
            println!("converting {linear} to {display}");
            let mut converted = ImageBuf::empty()?;
            // unpremult, because a colour transform is not linear and would
            // otherwise darken the edges of anything partly transparent.
            algo::color_convert(&mut converted, &fitted, &linear, &display, true, None)?;
            converted
        }
        None => {
            println!("no display colour space available; writing as-is");
            fitted
        }
    };

    let mut result = converted;
    result.write(std::path::Path::new(output))?;
    println!("wrote {output}");
    Ok(())
}

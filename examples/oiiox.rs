//! `oiiox`, an `oiiotool`-flavoured command chain over an image stack.
//!
//! Commands execute in the order written, over a stack of images: `-i` reads
//! a file and pushes it, and every other command inspects or replaces the top
//! of the stack. Each operation prints one line saying what it did. Any
//! failure is reported as `oiiox: <message>` on stderr with exit code 1, and
//! a `--diff` whose images differ beyond the failure threshold makes the run
//! exit with code 2 once the whole chain has executed.
//!
//! ```text
//! cargo run --example oiiox -- -i in.exr --info --stats --resize 512x288 --flip --o out.png
//! cargo run --example oiiox -- -i render.exr -i reference.exr --diff
//! ```

use std::path::Path;
use std::process::ExitCode;

use oiio::algo::{self, ChannelSource};
use oiio::{ImageBuf, ImageSpec};

/// The absolute per-channel difference beyond which `--diff` fails, matching
/// `oiiotool`'s default.
const FAIL_THRESHOLD: f32 = 1.0e-6;

/// The difference at which `--diff` merely warns.
const WARN_THRESHOLD: f32 = 1.0e-6;

/// Errors are already text by the time the chain unwinds: the message the
/// tool prints, without the exit machinery attached.
type CliResult<T> = Result<T, String>;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.is_empty()
        || arguments
            .iter()
            .any(|word| word == "--help" || word == "-h")
    {
        print_usage();
        return ExitCode::SUCCESS;
    }
    match run(&arguments) {
        Ok(false) => ExitCode::SUCCESS,
        // Every command ran, but a --diff comparison failed.
        Ok(true) => ExitCode::from(2),
        Err(message) => {
            eprintln!("oiiox: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Execute the command chain left to right. `Ok(true)` means every command
/// ran but at least one `--diff` failed.
fn run(arguments: &[String]) -> CliResult<bool> {
    let mut stack: Vec<ImageBuf> = Vec::new();
    let mut diff_failed = false;
    let mut words = arguments.iter();
    while let Some(command) = words.next() {
        match command.as_str() {
            "-i" => read_image(&mut stack, value(&mut words, "-i")?)?,
            "--o" => write_image(&mut stack, value(&mut words, "--o")?)?,
            "--info" => print_info(&stack)?,
            "--stats" => print_stats(&stack)?,
            "--resize" => resize(&mut stack, value(&mut words, "--resize")?)?,
            "--flip" => unary(&mut stack, "--flip", "flip", |dst, src| {
                algo::flip(dst, src, None)
            })?,
            "--flop" => unary(&mut stack, "--flop", "flop", |dst, src| {
                algo::flop(dst, src, None)
            })?,
            "--premult" => unary(&mut stack, "--premult", "premult", |dst, src| {
                algo::premult(dst, src, None)
            })?,
            "--unpremult" => unary(&mut stack, "--unpremult", "unpremult", |dst, src| {
                algo::unpremult(dst, src, None)
            })?,
            "--colorconvert" => {
                let from = value(&mut words, "--colorconvert")?;
                let to = value(&mut words, "--colorconvert")?;
                let label = format!("colorconvert {from} -> {to}");
                unary(&mut stack, "--colorconvert", &label, |dst, src| {
                    algo::color_convert(dst, src, from, to, true, None)
                })?;
            }
            "--ch" => shuffle_channels(&mut stack, value(&mut words, "--ch")?)?,
            "--diff" => diff_failed |= diff(&mut stack)?,
            other => {
                return Err(format!(
                    "unknown command {other:?}; run with --help for the list"
                ));
            }
        }
    }
    Ok(diff_failed)
}

/// The word after a command that requires one.
fn value<'a>(words: &mut std::slice::Iter<'a, String>, command: &str) -> CliResult<&'a str> {
    words
        .next()
        .map(String::as_str)
        .ok_or_else(|| format!("{command} needs a value; run with --help for usage"))
}

/// The crate reports typed errors; the tool prints them as text.
fn text<T>(result: oiio::Result<T>) -> CliResult<T> {
    result.map_err(|error| error.to_string())
}

/// The complaint for a command that found the stack too shallow.
fn missing(command: &str) -> String {
    format!("{command} needs an image on the stack; read one with -i first")
}

/// The top of the stack, which `command` is about to inspect.
fn top<'a>(stack: &'a [ImageBuf], command: &str) -> CliResult<&'a ImageBuf> {
    stack.last().ok_or_else(|| missing(command))
}

/// The top of the stack, mutably, for commands that touch it in place.
fn top_mut<'a>(stack: &'a mut [ImageBuf], command: &str) -> CliResult<&'a mut ImageBuf> {
    stack.last_mut().ok_or_else(|| missing(command))
}

/// Remove the top of the stack, which `command` is about to replace.
fn pop(stack: &mut Vec<ImageBuf>, command: &str) -> CliResult<ImageBuf> {
    stack.pop().ok_or_else(|| missing(command))
}

/// `-i`: attach to a file and push it. The specification is resolved here,
/// so a missing or unreadable file fails at this word of the chain rather
/// than at the first operation to want pixels.
fn read_image(stack: &mut Vec<ImageBuf>, path: &str) -> CliResult<()> {
    let image = text(ImageBuf::from_path(Path::new(path)))?;
    let spec = text(image.spec())?;
    let [width, height, _] = spec.dimensions();
    println!(
        "read {path}: {width}x{height}, {} channel(s), {}",
        spec.channel_count(),
        spec.format()
    );
    stack.push(image);
    Ok(())
}

/// `--o`: write the top image, letting the extension pick the file format.
/// The image stays on the stack, so a chain can write and keep going.
fn write_image(stack: &mut [ImageBuf], path: &str) -> CliResult<()> {
    let image = top_mut(stack, "--o")?;
    text(image.write(Path::new(path)))?;
    let [width, height, _] = text(image.spec())?.dimensions();
    println!("write {path}: {width}x{height}");
    Ok(())
}

/// `--info`: describe the top image without touching its pixels.
fn print_info(stack: &[ImageBuf]) -> CliResult<()> {
    let image = top(stack, "--info")?;
    let spec = text(image.spec())?;
    let [width, height, depth] = spec.dimensions();
    let name = if image.name().is_empty() {
        "(unnamed)"
    } else {
        image.name()
    };
    println!("info {name}");
    if depth > 1 {
        println!("  size: {width} x {height} x {depth}");
    } else {
        println!("  size: {width} x {height}");
    }
    println!("  pixel format: {}", spec.format());
    let channels: Vec<String> = spec
        .channel_names()
        .iter()
        .enumerate()
        .map(|(index, name)| {
            if spec.alpha_channel() == Some(index as u32) {
                format!("{name} (alpha)")
            } else {
                name.clone()
            }
        })
        .collect();
    println!(
        "  channels: {} [{}]",
        spec.channel_count(),
        channels.join(", ")
    );
    println!("  subimages: {}", image.subimage_count());
    Ok(())
}

/// `--stats`: per-channel measurements of the top image.
fn print_stats(stack: &[ImageBuf]) -> CliResult<()> {
    let image = top(stack, "--stats")?;
    let spec = text(image.spec())?;
    let stats = text(algo::pixel_stats(image, None))?;
    let [width, height, _] = spec.dimensions();
    println!("stats over {width}x{height}");
    // Zipping stops at the shortest sequence, so a mismatch between the
    // specification and the measurements cannot panic.
    let rows = spec
        .channel_names()
        .iter()
        .zip(&stats.min)
        .zip(&stats.max)
        .zip(&stats.average)
        .zip(&stats.standard_deviation)
        .zip(&stats.nan_count)
        .zip(&stats.infinite_count);
    for ((((((name, min), max), average), deviation), nan), infinite) in rows {
        println!("  {name}: min {min:.5} max {max:.5} avg {average:.5} stddev {deviation:.5}");
        if nan + infinite > 0 {
            println!("    {nan} NaN and {infinite} infinite value(s) excluded");
        }
    }
    Ok(())
}

/// `--resize`: filtered resize into a freshly allocated destination, which
/// is how the operation learns its output size. The destination keeps the
/// source's channel count, names and pixel format.
fn resize(stack: &mut Vec<ImageBuf>, size: &str) -> CliResult<()> {
    let (width, height) = parse_size(size)?;
    let source = pop(stack, "--resize")?;
    let spec = text(source.spec())?;
    let [source_width, source_height, _] = spec.dimensions();
    let target = ImageSpec::new(width, height, spec.channel_count(), spec.format())
        .and_then(|target| target.with_channel_names(spec.channel_names().iter().cloned()));
    let mut resized = text(target.and_then(|target| ImageBuf::new(&target)))?;
    text(algo::resize(&mut resized, &source, None, None, None))?;
    println!("resize {source_width}x{source_height} -> {width}x{height}");
    stack.push(resized);
    Ok(())
}

/// The `WxH` argument of `--resize`.
fn parse_size(size: &str) -> CliResult<(u32, u32)> {
    let invalid = || format!("--resize wants WxH with nonzero sides, e.g. 512x288, got {size:?}");
    let (width, height) = size.split_once(['x', 'X']).ok_or_else(invalid)?;
    let width: u32 = width.parse().map_err(|_| invalid())?;
    let height: u32 = height.parse().map_err(|_| invalid())?;
    if width == 0 || height == 0 {
        return Err(invalid());
    }
    Ok((width, height))
}

/// One destination-from-source operation replacing the top of the stack.
/// The destination starts empty and the operation shapes it from the source.
fn unary(
    stack: &mut Vec<ImageBuf>,
    command: &str,
    label: &str,
    operation: impl FnOnce(&mut ImageBuf, &ImageBuf) -> oiio::Result<()>,
) -> CliResult<()> {
    let source = pop(stack, command)?;
    let mut result = text(ImageBuf::empty())?;
    text(operation(&mut result, &source))?;
    let [width, height, _] = text(result.spec())?.dimensions();
    println!("{label} {width}x{height}");
    stack.push(result);
    Ok(())
}

/// `--ch`: rebuild the channel layout from a comma-separated list, each
/// entry a source channel name, a source channel index, or `name=constant`.
fn shuffle_channels(stack: &mut Vec<ImageBuf>, list: &str) -> CliResult<()> {
    let source = pop(stack, "--ch")?;
    let spec = text(source.spec())?;
    let names = spec.channel_names();
    let mut sources = Vec::new();
    let mut new_names: Vec<String> = Vec::new();
    for token in list.split(',') {
        let token = token.trim();
        if token.is_empty() {
            return Err("--ch has an empty entry; write it like --ch R,G,B".to_owned());
        }
        if let Some((name, constant)) = token.split_once('=') {
            if name.is_empty() {
                return Err(format!("--ch constant {token:?} needs a name, like A=1.0"));
            }
            let constant: f32 = constant
                .parse()
                .map_err(|_| format!("--ch constant {token:?} is not a number"))?;
            sources.push(ChannelSource::Constant(constant));
            new_names.push(name.to_owned());
        } else if let Ok(index) = token.parse::<u32>() {
            if index >= spec.channel_count() {
                return Err(format!(
                    "--ch index {index} is out of range: the image has {} channel(s)",
                    spec.channel_count()
                ));
            }
            sources.push(ChannelSource::Channel(index));
            new_names.push(
                names
                    .get(index as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("channel{index}")),
            );
        } else if let Some(index) = names.iter().position(|name| name.as_str() == token) {
            sources.push(ChannelSource::Channel(index as u32));
            new_names.push(token.to_owned());
        } else {
            return Err(format!(
                "--ch does not recognise {token:?}; the channels here are [{}]",
                names.join(", ")
            ));
        }
    }
    let mut result = text(ImageBuf::empty())?;
    let name_slices: Vec<&str> = new_names.iter().map(String::as_str).collect();
    text(algo::channels(
        &mut result,
        &source,
        &sources,
        Some(&name_slices),
    ))?;
    println!("ch [{}] -> [{}]", names.join(", "), new_names.join(", "));
    stack.push(result);
    Ok(())
}

/// `--diff`: compare the two topmost images and pop the later one, so a
/// chain can hold a reference underneath while candidates come and go.
/// Returns whether the comparison failed.
fn diff(stack: &mut Vec<ImageBuf>) -> CliResult<bool> {
    if stack.len() < 2 {
        return Err("--diff needs two images on the stack; read them with -i first".to_owned());
    }
    let candidate = pop(stack, "--diff")?;
    let reference = top(stack, "--diff")?;
    let summary = text(algo::compare(
        reference,
        &candidate,
        FAIL_THRESHOLD,
        WARN_THRESHOLD,
        None,
    ))?;
    let verdict = if summary.failed {
        "FAIL"
    } else if summary.warnings > 0 {
        "WARN"
    } else {
        "PASS"
    };
    println!(
        "diff mean {:.6} rms {:.6} max {:.6} at ({}, {}) channel {}",
        summary.mean_error,
        summary.root_mean_square_error,
        summary.max_error,
        summary.max_x,
        summary.max_y,
        summary.max_channel
    );
    println!(
        "diff {} value(s) warned, {} failed: {verdict}",
        summary.warnings, summary.failures
    );
    Ok(summary.failed)
}

/// The full command list, printed for `--help` and for an empty invocation.
fn print_usage() {
    println!(
        "\
oiiox: an oiiotool-flavoured command chain over an image stack

Commands run left to right. -i pushes an image onto the stack; every other
command inspects or replaces the top of it. An error stops the chain with
exit code 1; a --diff beyond the failure threshold exits 2 after the chain
has finished.

  -i <file>                  read an image and push it on the stack
  --o <file>                 write the top image; the extension picks the format
  --info                     print the top image's specification
  --stats                    print per-channel min/max/average/stddev
  --resize <WxH>             filtered resize, e.g. --resize 512x288
  --flip                     mirror top-to-bottom
  --flop                     mirror left-to-right
  --premult                  multiply colour channels by alpha
  --unpremult                divide colour channels by alpha
  --colorconvert <from> <to> convert colour spaces, e.g. srgb lin_srgb
  --ch <list>                reorder channels by name, index or name=constant,
                             e.g. --ch R,G,B or --ch 2,1,0,A=1.0
  --diff                     compare the two topmost images and pop the later
  --help                     print this text

examples:
  oiiox -i in.exr --info --stats --resize 512x288 --flip --o out.png
  oiiox -i render.exr -i reference.exr --diff"
    );
}

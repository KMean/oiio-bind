# oiio-viewer

A minimal image and image-sequence viewer demonstrating the `oiio` crate.
Frames are decoded to linear f32 by OpenImageIO, exposure-adjusted,
sRGB-encoded, and presented on the CPU with `winit` and `softbuffer`.
It lives outside the main workspace so the library's dependency graph
stays free of windowing crates.

## Running

    cargo run --release -- image1.exr image2.exr   # explicit sequence
    cargo run --release -- path/to/directory       # all image files, sorted
    cargo run --release -- --check image.exr       # decode only, print WxHxC

## Keys

| Key           | Action                              |
|---------------|-------------------------------------|
| Right / Left  | next / previous frame (wraps)       |
| + / - (= / _) | exposure up / down one stop         |
| Home          | reset exposure                      |
| R             | reload the current file from disk   |
| Esc / Q       | quit                                |

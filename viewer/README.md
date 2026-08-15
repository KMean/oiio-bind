# oiio-viewer

A minimal image and image-sequence viewer demonstrating the `oiio` crate.
Frames are decoded by OpenImageIO and normalised to linear f32 — files that
store display-encoded pixels, such as JPEG and PNG, are linearised on load —
then exposure-adjusted, sRGB-encoded, and presented on the CPU with `winit`
and `softbuffer`. Multi-part EXR files can be flipped through part by part.
It lives outside the main workspace so the library's dependency graph
stays free of windowing crates.

## Running

    cargo run --release -- image1.exr image2.exr   # explicit sequence
    cargo run --release -- path/to/directory       # sorted sequence
    cargo run --release -- --check image.exr       # decode only, print WxHxC

A directory that mixes formats — EXR renders next to JPEG previews — forms
its sequence from the dominant extension (EXR wins ties) rather than
interleaving formats.

## Keys

| Key           | Action                               |
|---------------|--------------------------------------|
| Right / Left  | next / previous frame (wraps)        |
| + / - (= / _) | exposure up / down one stop          |
| Home          | reset exposure                       |
| S             | next subimage of a multi-part file   |
| R             | reload the current file from disk    |
| Esc / Q       | quit                                 |

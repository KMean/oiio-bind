# oiio-viewer

An image and image-sequence viewer demonstrating the `oiio` crate as a small
review tool.

**This is an example of the crate's use, not a review-tool product.** The
code favours clarity over throughput: every channel decodes to linear
`f32` even when four of a hundred reach the screen, the only cache is a
small in-memory ring around the playhead, and the display transform runs
on the CPU. Heavy multichannel sequences therefore play slowly — by
design, not by accident. A real player built on `oiio` would read just
the viewed layer through the crate's channel-subset reads, keep pixels in
half float, prefetch aggressively, and hand the display transform to the
GPU. The same API supports all of that; this example deliberately stays
simple enough to read in a sitting. Frames are decoded by OpenImageIO to linear f32 — files that
store display-encoded pixels, such as JPEG and PNG, are linearised on load —
then exposure-adjusted, channel-isolated and sRGB-encoded for display with
`eframe`/`egui`. The window carries playback controls with a frame scrubber,
cursor-centred zoom and pan, and an inspector panel for the current frame's
spec and metadata; multi-part EXR files can be flipped through part by part.
A frame that fails to decode shows as a placeholder with the error text, and
playback runs on past it. The crate lives outside the main workspace so the
library's dependency graph stays free of windowing crates.

<!-- screenshot: docs/screenshot.png — the viewer on a multi-part EXR with
     the inspector panel open -->

## Running

    cargo run --release                            # empty window; open from it
    cargo run --release -- image1.exr image2.exr   # explicit sequence
    cargo run --release -- path/to/directory       # the directory's sequence
    cargo run --release -- path/to/shot.#.exr      # one named sequence
    cargo run --release -- --check image.exr       # decode only, print WxHxC

Sequences also open from inside the window: File → Open files/folder,
`Ctrl+O`, or drop images — or a folder — onto it. A dropped or picked
folder resolves exactly as a directory argument does.

A directory argument resolves to one sequence, the way `oiiotool` thinks of
them: image files of the dominant extension (EXR wins ties), then the
largest name pattern among them — `beauty.####.exr` next to `depth.####.exr`
is two sequences, and only one plays, with a notice naming what was set
aside. A `#` pattern names a sequence directly, matching frame digits of
any width. Frame numbers order numerically whether zero-padded or not, and
a folder with no numbering at all stays browsable whole.

## Keys

| Key           | Action                                     |
|---------------|--------------------------------------------|
| Ctrl+O        | open files                                 |
| Space         | play / pause                               |
| L             | next layer of a multichannel image         |
| Right / Left  | next / previous frame (wraps)              |
| Up / Down, S  | next / previous part of a multi-part file  |
| + / - (= / _) | exposure up / down one stop                |
| Home          | reset exposure                             |
| F             | fit the image to the window                |
| 1             | one image pixel per screen pixel           |
| R             | reload the current frame from disk         |
| Esc / Q       | quit                                       |

The bottom bar carries the same transport as buttons, a frame scrubber and
the playback rate; the right panel holds exposure, channel isolation
(RGB/R/G/B/A/Luma), a layer dropdown for multichannel images, the current
frame's spec with a part selector, and the file's metadata table.

A Nuke-style multichannel EXR groups into layers by its channel names —
`ID01.red` is the `red` of layer `ID01` — with components ordered by
suffix, undotted depth (`Z`, `ZBack`) kept out of the colour slots, and a
layer's own `alpha` respected. Single-channel layers show as grey, and
switching layers reuses the decoded pixels, so it is as instant as
exposure.

## Design

Decoding runs on a background thread feeding a small LRU cache — twelve
frames or 1.5 GB of pixels, whichever binds first — and the frames ahead of
the playhead (behind it when stepping backwards) are prefetched so playback
does not stall on I/O. Requests made stale by a scrub jump or a reload are
skipped for the cost of an atomic load, and the newest live request is
served first, so dragging the scrubber across a heavy sequence never waits
on a backlog of decodes nobody wants. The display transform runs on the UI
thread from the cached linear pixels, so exposure and channel changes are
instant and never touch the disk.

An EXR whose data window crops into or overscans past its display window is
framed by the display window, with the pixels placed where they belong
inside it — the display window drawn as a solid outline and the data window
as a dashed bounding box, the way review tools mark them. Images larger than the
GPU's texture limit are subsampled for display rather than refused.

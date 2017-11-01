# [gif.ski](https://gif.ski)

Highest-quality GIF encoder based on [pngquant](https://pngquant.org).

**[gifski](https://gif.ski)** converts video frames to GIF animations using pngquant's fancy features for efficient cross-frame palettes and temporal dithering. It produces animated GIFs that use thousands of colors per frame.

![(CC) Blender Foundation | gooseberry.blender.org](https://gif.ski/demo.gif)

It's a CLI tool, but it can also be compiled as library for seamelss use in other apps (note that for closed-source apps you need a commercial pngquant license).

## Download and install

See [releases](https://github.com/ImageOptim/gifski/releases) page for executables.

If you have Rust, you can also get it with `cargo install gifski`. Run `cargo build --release` to build from suorce.

## Usage

I haven't finished implementing proper video import yet, so for now you need `ffmpeg` to convert video to PNG frames first:

```sh
ffmpeg -i video.mp4 frame%04d.png
```

and then make the GIF from the frames:

```sh
gifski -o file.gif frame*.png
```

See `gifski -h` for more options.


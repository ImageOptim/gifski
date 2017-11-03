# [gif.ski](https://gif.ski)

Highest-quality GIF encoder based on [pngquant](https://pngquant.org).

**[gifski](https://gif.ski)** converts video frames to GIF animations using pngquant's fancy features for efficient cross-frame palettes and temporal dithering. It produces animated GIFs that use thousands of colors per frame.

![(CC) Blender Foundation | gooseberry.blender.org](https://gif.ski/demo.gif)

It's a CLI tool, but it can also be compiled as library for seamelss use in other apps (note that for closed-source apps you need a commercial pngquant license).

## Download and install

See [releases](https://github.com/ImageOptim/gifski/releases) page for executables.

If you have Rust, you can also get it with `cargo install gifski`. Run `cargo build --release` to build from suorce.

## Usage

You can use `ffmpeg` command to convert any video to PNG frames:

```sh
ffmpeg -i video.mp4 frame%04d.png
```

and then make the GIF from the frames:

```sh
gifski -o file.gif frame*.png
```

You can also resize frames (with `-W <width in pixels>` option). If the input was ever encoded using a lossy video codec it's recommended to at least halve size of the frames to hide compression artefacts and counter chroma subsampling that was done by the video codec.

See `gifski -h` for more options.

## License

AGPL 3 or later. Let [me](https://kornel.ski/contact) know if you'd like to use it in a product incompatible with this license. I can offer alternative licensing options.

## Building with built-in video support

Compile with `cargo build --release --features=video`.

Video support requires ffmpeg library. When compiled with video support [ffmpeg licenses](https://www.ffmpeg.org/legal.html) apply. You may need to have a patent license to use H.264/H.265 video (I recommend using VP9/WebM instead).

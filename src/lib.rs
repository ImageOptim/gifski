extern crate threadpool;
extern crate rgb;
extern crate gif;
extern crate imgref;
extern crate imagequant;
extern crate lodepng;
extern crate gif_dispose;
extern crate rayon;

#[macro_use] extern crate error_chain;
use gif::*;
use rgb::*;
use imgref::*;
use imagequant::*;

mod error;
pub use error::*;
mod ordparqueue;
use ordparqueue::*;

use std::path::PathBuf;
use std::io::prelude::*;
use rayon::prelude::*;

type DecodedImage = CatResult<(ImgVec<RGBA8>, u16)>;

pub struct Collector {
    queue: OrdParQueue<DecodedImage>,
}

pub struct Writer {
    queue_iter: OrdParQueueIter<DecodedImage>,
}

/// Encoder is initialized after first frame is decoded,
/// and this explains to Rust that writer `W` is used once for this.
enum WriteInitState<W: Write> {
    Uninit(W),
    Init(Encoder<W>)
}

pub fn new() -> CatResult<(Collector, Writer)> {
    let (queue, queue_iter) = ordparqueue::new(8);

    Ok((Collector {queue}, Writer {queue_iter}))
}

/// Collect frames that will be encoded
impl Collector {

    pub fn add_frame_png_file(&mut self, path: PathBuf, delay: u16) {
        // Frames are decoded async in a queue
        self.queue.push(move || {
            let image = lodepng::decode32_file(&path)
                .chain_err(|| format!("Can't load {}", path.display()))?;
            Ok((ImgVec::new(image.buffer, image.width, image.height), delay))
        });
    }
}

/// Encode collected frames
impl Writer {

    /// `importance_map` is computed from previous and next frame.
    /// Improves quality of pixels visible for longer.
    /// Avoids wasting palette on pixels identical to the background.
    ///
    /// `background` is the previous frame.
    fn quantize(image: ImgRef<RGBA8>, importance_map: &[u8], background: Option<ImgRef<RGBA8>>) -> CatResult<(ImgVec<u8>, Vec<RGBA8>)> {
        let mut liq = Attributes::new();
        let mut img = liq.new_image(image.buf, image.width(), image.height(), 0.)?;
        img.set_importance_map(importance_map)?;
        if let Some(bg) = background {
            img.set_background(liq.new_image(bg.buf, bg.width(), bg.height(), 0.)?)?;
        }
        img.add_fixed_color(RGBA8::new(0,0,0,0));
        let mut res = liq.quantize(&img)?;
        res.set_dithering_level(0.5);

        let (pal, pal_img) = res.remapped(&mut img)?;
        debug_assert_eq!(img.width() * img.height(), pal_img.len());

        Ok((Img::new(pal_img, img.width(), img.height()), pal))
    }

    fn write_gif_frame<W: Write>(image: ImgRef<u8>, pal: &[RGBA8], transparent_index: Option<u8>, delay: u16, enc: &mut Encoder<W>) -> CatResult<()> {
        let mut pal_rgb = Vec::with_capacity(3 * pal.len());
        for p in pal {
            pal_rgb.extend_from_slice(p.rgb().as_bytes());
        }

        enc.write_frame(&Frame {
            delay,
            dispose: DisposalMethod::Keep,
            transparent: transparent_index,
            needs_user_input: false,
            top: 0,
            left: 0,
            width: image.width as u16,
            height: image.height as u16,
            interlaced: false,
            palette: Some(pal_rgb),
            buffer: image.buf.into(),
        })?;
        Ok(())
    }

    pub fn write<W: Write + Send>(self, outfile: W, once: bool) -> CatResult<()> {
        let mut decode_iter = self.queue_iter.enumerate().map(|(i,tmp)| tmp.map(|(image, delay)|(i,image,delay)));

        let mut screen = None;
        let mut curr_frame = if let Some(a) = decode_iter.next() {
            Some(a?)
        } else {
            Err("no frames")?
        };
        let mut next_frame = if let Some(a) = decode_iter.next() {
            Some(a?)
        } else {
            None
        };

        let mut enc = WriteInitState::Uninit(outfile);
        while let Some((i, image, delay)) = curr_frame.take() {
            println!("frame {}", i);
            curr_frame = next_frame.take();
            next_frame = if let Some(a) = decode_iter.next() {
                Some(a?)
            } else {
                None
            };

            let has_prev_frame = i > 0;

            let mut importance_map: Vec<u8> = if let Some((_, ref next, _)) = next_frame {
                debug_assert_eq!(next.stride(), image.stride());
                next.buf.par_iter().cloned().zip(image.buf.par_iter().cloned()).map(|(a,b)| {
                    // Even if next frame completely overwrites it, it's still somewhat important to display current one
                    // but pixels that will stay unchanged should have higher quality
                    255 - (colordiff(a,b) / (255*255*6/170)) as u8
                }).collect()
            } else {
                vec![255; image.buf.len()]
            };

            if screen.is_none() {
                screen = Some(gif_dispose::Screen::new(image.width(), image.height(), RGBA8::new(0,0,0,0), None));
            }
            let screen = screen.as_mut().unwrap();

            if has_prev_frame {
                debug_assert_eq!(screen.pixels.stride(), image.stride());
                importance_map.par_iter_mut().zip(screen.pixels.buf.par_iter().cloned().zip(image.buf.par_iter().cloned()))
                .for_each(|(px, (a,b))| {
                    // TODO: try comparing with max-quality dithered non-transparent frame, but at half res to avoid dithering confusing the results
                    // and pick pixels/areas that are better left transparent?
                    let min_diff = 16;

                    let diff = colordiff(a,b);
                    // if pixels are close or identical, no weight on them
                    *px = if diff < min_diff {
                        0
                    } else {
                        // clip max value, since if something's different it doesn't matter how much, it has to be displayed anyway
                        // but multiply by previous map last, since it already decided non-max value
                        let t = diff / 32;
                        ((t * t).min(256) as u16 * u16::from(*px) / 256) as u8
                    }
                });
            }

            let (image8, image8_pal) = {
                let bg = if has_prev_frame {Some(screen.pixels.as_ref())} else {None};
                Self::quantize(image.as_ref(), &importance_map, bg)?
            };

            enc = match enc {
                WriteInitState::Uninit(w) => {
                    let mut enc = Encoder::new(w, image8.width as u16, image8.height as u16, &[])?;
                    if !once {
                        enc.write_extension(gif::ExtensionData::Repetitions(gif::Repeat::Infinite))?;
                    }
                    WriteInitState::Init(enc)
                },
                x => x,
            };
            let enc = match enc {
                WriteInitState::Init(ref mut r) => r,
                _ => unreachable!(),
            };

            let transparent_index = image8_pal.iter().position(|p| p.a == 0).map(|i| i as u8);
            let (blit_res, enc_res) = rayon::join(|| {
                screen.blit(Some(&image8_pal), gif::DisposalMethod::Keep, 0, 0, image8.as_ref(), transparent_index)
            }, || {
                Self::write_gif_frame(image8.as_ref(), &image8_pal, transparent_index, delay, enc)
            });
            blit_res?; enc_res?;
        }

        Ok(())
    }
}

#[inline]
fn colordiff(a: RGBA8, b: RGBA8) -> u32 {
    if a.a == 0 || b.a == 0 {
        return 255*255*6;
    }
    (i32::from(a.r as i16 - b.r as i16) * i32::from(a.r as i16 - b.r as i16)) as u32 * 2 +
    (i32::from(a.g as i16 - b.g as i16) * i32::from(a.g as i16 - b.g as i16)) as u32 * 3 +
    (i32::from(a.b as i16 - b.b as i16) * i32::from(a.b as i16 - b.b as i16)) as u32
}

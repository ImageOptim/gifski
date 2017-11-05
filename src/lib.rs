/*
 gifski pngquant-based GIF encoder
 © 2017 Kornel Lesiński

 This program is free software: you can redistribute it and/or modify
 it under the terms of the GNU Affero General Public License as
 published by the Free Software Foundation, either version 3 of the
 License, or (at your option) any later version.

 This program is distributed in the hope that it will be useful,
 but WITHOUT ANY WARRANTY; without even the implied warranty of
 MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 GNU Affero General Public License for more details.

 You should have received a copy of the GNU Affero General Public License
 along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

extern crate threadpool;
extern crate rgb;
extern crate gif;
extern crate imgref;
extern crate imagequant;
extern crate resize;
extern crate lodepng;
extern crate gif_dispose;
extern crate rayon;
extern crate pbr;

#[macro_use] extern crate error_chain;
use gif::*;
use rgb::*;
use imgref::*;
use imagequant::*;

mod error;
pub use error::*;
mod ordparqueue;
use ordparqueue::*;
pub mod progress;
use progress::*;

use std::path::PathBuf;
use std::io::prelude::*;
use rayon::prelude::*;

type DecodedImage = CatResult<(ImgVec<RGBA8>, u16)>;

pub struct Settings {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub quality: u8,
    pub once: bool,
    pub fast: bool,
}

pub struct Collector {
    pub width: Option<u32>,
    pub height: Option<u32>,
    queue: OrdParQueue<DecodedImage>,
}

pub struct Writer {
    queue_iter: OrdParQueueIter<DecodedImage>,
    settings: Settings,
}

/// Encoder is initialized after first frame is decoded,
/// and this explains to Rust that writer `W` is used once for this.
enum WriteInitState<W: Write> {
    Uninit(W),
    Init(Encoder<W>),
}

pub fn new(settings: Settings) -> CatResult<(Collector, Writer)> {
    let (queue, queue_iter) = ordparqueue::new("decoding".to_string(), 4);

    Ok((Collector {
        queue,
        width: settings.width,
        height: settings.height,
    }, Writer {
        queue_iter,
        settings,
    }))
}

/// Collect frames that will be encoded
impl Collector {
    pub fn fail<E: Into<Error>>(mut self, err: E) {
        self.queue.push_sync(Err(err.into())).expect("Failed so hard it can't even report failure");
    }

    pub fn add_frame_rgba_sync(&mut self, image: ImgVec<RGBA8>, delay: u16) -> CatResult<()> {
        self.queue.push_sync(Ok((Self::resized(image, self.width, self.height), delay)))?;
        Ok(())
    }

    pub fn add_frame_png_file(&mut self, path: PathBuf, delay: u16) {
        // Frames are decoded async in a queue
        let width = self.width;
        let height = self.height;
        self.queue.push(move || {
            let image = lodepng::decode32_file(&path)
                .chain_err(|| format!("Can't load {}", path.display()))?;

            Ok((Self::resized(ImgVec::new(image.buffer, image.width, image.height), width, height), delay))
        });
    }

    fn resized(image: ImgVec<RGBA8>, width: Option<u32>, height: Option<u32>) -> ImgVec<RGBA8> {
        if let Some(width) = width {
            assert_eq!(image.width(), image.stride());
            let dst_width = (width as usize).min(image.width());
            let dst_height = height.map(|h| (h as usize).min(image.height())).unwrap_or(image.height() * dst_width / image.width());
            let mut r = resize::new(image.width(), image.height(), dst_width, dst_height, resize::Pixel::RGBA, resize::Type::Lanczos3);
            let mut dst = vec![RGBA::new(0,0,0,0); dst_width * dst_height];
            r.resize(image.buf.as_bytes(), dst.as_bytes_mut());
            ImgVec::new(dst, dst_width, dst_height)
        } else {
            image
        }
    }
}

/// Encode collected frames
impl Writer {

    /// `importance_map` is computed from previous and next frame.
    /// Improves quality of pixels visible for longer.
    /// Avoids wasting palette on pixels identical to the background.
    ///
    /// `background` is the previous frame.
    fn quantize(image: ImgRef<RGBA8>, importance_map: &[u8], background: Option<ImgRef<RGBA8>>, settings: &Settings) -> CatResult<(ImgVec<u8>, Vec<RGBA8>)> {
        let mut liq = Attributes::new();
        if settings.fast {
            liq.set_speed(10);
        }
        let quality = if background.is_some() { // not first frame
            settings.quality.into()
        } else {
            100 // the first frame is too important to ruin it
        };
        liq.set_quality(0, quality);
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
            pal_rgb.extend_from_slice([p.rgb()].as_bytes());
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

    pub fn write<W: Write + Send>(self, outfile: W, reporter: &mut ProgressReporter) -> CatResult<()> {
        let mut decode_iter = self.queue_iter.enumerate().map(|(i,tmp)| tmp.map(|(image, delay)|(i,image,delay)));

        let mut screen = None;
        let mut curr_frame = if let Some(a) = decode_iter.next() {
            Some(a?)
        } else {
            Err("Found no usable frames to encode")?
        };
        let mut next_frame = if let Some(a) = decode_iter.next() {
            Some(a?)
        } else {
            None
        };

        let mut enc = WriteInitState::Uninit(outfile);
        while let Some((i, image, delay)) = curr_frame.take() {
            reporter.increase();
            curr_frame = next_frame.take();
            next_frame = if let Some(a) = decode_iter.next() {
                Some(a?)
            } else {
                None
            };

            let has_prev_frame = i > 0;

            let mut importance_map: Vec<u8> = if let Some((_, ref next, _)) = next_frame {

                if next.width() != image.width() || next.height() != image.height() {
                    Err(format!("Frame {} has wrong size ({}×{}, expected {}×{})", i+1,
                        next.width(), next.height(), image.width(), image.height()))?;
                }

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
                let q = 100 - self.settings.quality as u32;
                let min_diff = 80 + q * q;
                importance_map.par_iter_mut().zip(screen.pixels.buf.par_iter().cloned().zip(image.buf.par_iter().cloned()))
                .for_each(|(px, (a,b))| {
                    // TODO: try comparing with max-quality dithered non-transparent frame, but at half res to avoid dithering confusing the results
                    // and pick pixels/areas that are better left transparent?

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
                Self::quantize(image.as_ref(), &importance_map, bg, &self.settings)?
            };

            enc = match enc {
                WriteInitState::Uninit(w) => {
                    let mut enc = Encoder::new(w, image8.width as u16, image8.height as u16, &[])?;
                    if !self.settings.once {
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

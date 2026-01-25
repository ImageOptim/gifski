use wasm_bindgen::prelude::*;
use crate::{Settings, Repeat};
use imgref::ImgVec;
use rgb::RGBA8;
use std::sync::Arc;
use std::sync::Mutex;

#[wasm_bindgen]
pub struct GifskiWasm {
    settings: Settings,
    frames: Vec<(ImgVec<RGBA8>, f64)>,
    width: u32,
    height: u32,
}

#[wasm_bindgen]
impl GifskiWasm {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32, quality: u8) -> Result<GifskiWasm, JsValue> {
        // Better panic messages in browser console
        console_error_panic_hook::set_once();
        
        let settings = Settings {
            width: if width > 0 { Some(width) } else { None },
            height: if height > 0 { Some(height) } else { None },
            quality,
            fast: false,
            repeat: Repeat::Infinite,
        };
        
        Ok(GifskiWasm { 
            settings,
            frames: Vec::new(),
            width,
            height,
        })
    }
    
    pub fn add_frame_rgba(&mut self, rgba_data: &[u8], width: u32, height: u32, timestamp: f64) -> Result<(), JsValue> {
        if rgba_data.len() != (width * height * 4) as usize {
            return Err(JsValue::from_str("Invalid RGBA data size"));
        }
        
        // flat RGBA bytes to RGBA8 pixels
        if width != self.width || height != self.height {
            return Err(JsValue::from_str("Frame dimensions don't match"));
        }
        
        let pixels: Vec<RGBA8> = rgba_data
            .chunks_exact(4)
            .map(|chunk| RGBA8::new(chunk[0], chunk[1], chunk[2], chunk[3]))
            .collect();
        
        let img = ImgVec::new(pixels, width as usize, height as usize);
        
        self.frames.push((img, timestamp));
        
        Ok(())
    }
    
    pub fn finish(self) -> Result<Vec<u8>, JsValue> {
        if self.frames.is_empty() {
            return Err(JsValue::from_str("No frames added"));
        }
        
        let (collector, writer) = crate::new(self.settings)
            .map_err(|e| JsValue::from_str(&format!("Failed to create gifski: {:?}", e)))?;
        
        let frames = self.frames;
        let output = Arc::new(Mutex::new(Vec::new()));
        let output_clone = output.clone();
        
        let writer_handle = std::thread::spawn(move || {
            let mut output_guard = output_clone.lock().unwrap();
            writer.write(&mut *output_guard, &mut crate::progress::NoProgress {})
        });
        
        for (frame_index, (img, timestamp)) in frames.into_iter().enumerate() {
            collector.add_frame_rgba(frame_index, img, timestamp)
                .map_err(|e| JsValue::from_str(&format!("Failed to add frame {}: {:?}", frame_index, e)))?;
        }
        
        drop(collector);
        
        writer_handle.join()
            .map_err(|_| JsValue::from_str("Writer thread panicked"))?
            .map_err(|e| JsValue::from_str(&format!("Failed to write GIF: {:?}", e)))?;
        
        let output_vec = Arc::try_unwrap(output)
            .map_err(|_| JsValue::from_str("Failed to unwrap output"))?
            .into_inner()
            .map_err(|_| JsValue::from_str("Failed to lock output"))?;
        
        Ok(output_vec)
    }
}

// thread pool for wasm-bindgen-rayon
#[wasm_bindgen]
pub fn init_thread_pool(num_threads: usize) -> Result<(), JsValue> {
    wasm_bindgen_rayon::init_thread_pool(num_threads);
    Ok(())
}
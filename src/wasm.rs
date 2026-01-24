use wasm_bindgen::prelude::*;
use crate::{Settings, Repeat};

#[wasm_bindgen]
pub struct GifskiWasm {
    collector: crate::Collector,
    writer: crate::Writer,
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
        
        let (collector, writer) = crate::new(settings)
            .map_err(|e| JsValue::from_str(&format!("Failed to create gifski: {:?}", e)))?;
        
        Ok(GifskiWasm { collector, writer })
    }
    
    pub fn add_frame_rgba(&mut self, rgba_data: &[u8], width: u32, height: u32, timestamp: f64) -> Result<(), JsValue> {
        use imgref::ImgVec;
        use rgb::RGBA8;
        
        if rgba_data.len() != (width * height * 4) as usize {
            return Err(JsValue::from_str("Invalid RGBA data size"));
        }
        
        // flat RGBA bytes to RGBA8 pixels
        let pixels: Vec<RGBA8> = rgba_data
            .chunks_exact(4)
            .map(|chunk| RGBA8::new(chunk[0], chunk[1], chunk[2], chunk[3]))
            .collect();
        
        let img = ImgVec::new(pixels, width as usize, height as usize);
        
        self.collector.add_frame_rgba(0, img, timestamp)
            .map_err(|e| JsValue::from_str(&format!("Failed to add frame: {:?}", e)))?;
        
        Ok(())
    }
    
    pub fn finish(self) -> Result<Vec<u8>, JsValue> {
        // Drop collector to signal no more frames
        drop(self.collector);
        
        let mut output = Vec::new();
        self.writer.write(&mut output, &mut crate::progress::NoProgress {})
            .map_err(|e| JsValue::from_str(&format!("Failed to write GIF: {:?}", e)))?;
        
        Ok(output)
    }
}
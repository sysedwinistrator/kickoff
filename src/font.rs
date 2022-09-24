use crate::{color::Color, config::Config};
use fontconfig::Fontconfig;
use fontdue::layout::{CoordinateSystem, GlyphRasterConfig, Layout, LayoutSettings, TextStyle};
use fontdue::Metrics;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use tokio::io;
use tokio::task::{spawn_blocking, JoinHandle};

use image::{Pixel, RgbaImage};

pub struct Font {
    fonts: Vec<fontdue::Font>,
    layout: RefCell<Layout>,
    scale: f32,
    glyph_cache: RefCell<HashMap<GlyphRasterConfig, (Metrics, Vec<u8>)>>,
}

impl TryFrom<&Config> for Font {
    type Error = std::io::Error;

    fn try_from(config: &Config) -> Result<Self, Self::Error> {
        if let Some(font_name) = config.font.as_ref() {
            let mut font_names = config.fonts.clone();
            font_names.insert(0, font_name.to_owned());
            Self::new(font_names, config.font_size)
        } else {
            Self::new(config.fonts.clone(), config.font_size)
        }
    }
}

impl Font {
    pub fn from_config(config: &Config) -> JoinHandle<Result<Font, std::io::Error>> {
        let font_names = if let Some(font_name) = config.font.as_ref() {
            let mut font_names = config.fonts.clone();
            font_names.insert(0, font_name.to_owned());
            font_names
        } else {
            config.fonts.clone()
        };

        let font_size = config.font_size.clone();
        spawn_blocking(move || Font::new(font_names, font_size))
    }

    pub fn new(font_names: Vec<String>, size: f32) -> io::Result<Font> {
        let fc = Fontconfig::new().expect("Couldn't load fontconfig");
        let font_names = if font_names.is_empty() {
            vec![String::new()]
        } else {
            font_names
        };
        let font_paths: Vec<PathBuf> = font_names
            .iter()
            .map(|name| fc.find(name, None).unwrap().path)
            .collect();
        let mut font_data = Vec::new();

        for font_path in font_paths {
            let mut font_buffer = Vec::new();
            File::open(font_path.to_str().unwrap())
                .expect("Failed to read font file")
                .read_to_end(&mut font_buffer)?;
            font_data.push(
                fontdue::Font::from_bytes(font_buffer, fontdue::FontSettings::default()).unwrap(),
            );
        }

        Ok(Font {
            fonts: font_data,
            layout: RefCell::new(Layout::new(CoordinateSystem::PositiveYDown)),
            scale: size,
            glyph_cache: RefCell::new(HashMap::new()),
        })
    }

    fn render_glyph(&self, conf: GlyphRasterConfig) -> (Metrics, Vec<u8>) {
        let mut glyph_cache = self.glyph_cache.borrow_mut();
        if let Some(bitmap) = glyph_cache.get(&conf) {
            bitmap.clone()
        } else {
            let font: Vec<&fontdue::Font> = self
                .fonts
                .iter()
                .filter(|f| (*f).file_hash() == conf.font_hash)
                .collect();
            glyph_cache.insert(conf, font.first().unwrap().rasterize_config(conf));
            glyph_cache.get(&conf).unwrap().clone()
        }
    }

    pub fn render(
        &mut self,
        text: &str,
        color: &Color,
        image: &mut RgbaImage,
        x_offset: u32,
        y_offset: u32,
    ) -> (u32, u32) {
        let mut width = 0;
        let mut layout = self.layout.borrow_mut();
        layout.reset(&LayoutSettings::default());
        for c in text.chars() {
            let mut font_index = 0;
            for (i, font) in self.fonts.iter().enumerate() {
                if font.lookup_glyph_index(c) != 0 {
                    font_index = i;
                    break;
                }
            }
            layout.append(
                &self.fonts,
                &TextStyle::new(&c.to_string(), self.scale, font_index),
            );
        }

        for glyph in layout.glyphs() {
            let (_, bitmap) = self.render_glyph(glyph.key);
            for (i, alpha) in bitmap.iter().enumerate() {
                if alpha != &0 {
                    let x = glyph.x + x_offset as f32 + (i % glyph.width) as f32;
                    let y = glyph.y + y_offset as f32 + (i / glyph.width) as f32;

                    image
                        .get_pixel_mut(x as u32, y as u32)
                        .blend(&image::Rgba([color.0, color.1, color.2, *alpha]));
                }
            }
        }
        if let Some(glyph) = layout.glyphs().last() {
            width = glyph.x as usize + glyph.width;
        }

        (width as u32, layout.height() as u32)
    }
}

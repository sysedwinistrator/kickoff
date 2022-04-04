#![warn(missing_docs)]
use image::Rgba;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::str::FromStr;

/// Wrapper for RGBA Colors
/// 
/// Can be build from the css_color::Rgba module and converted into Rgba<u8> 
#[derive(Clone)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl From<css_color::Rgba> for Color {
    fn from(c: css_color::Rgba) -> Self {
        Color(
            (c.red * 255.) as u8,
            (c.green * 255.) as u8,
            (c.blue * 255.) as u8,
            (c.alpha * 255.) as u8,
        )
    }
}

impl From<Color> for Rgba<u8> {
    fn from(c: Color) -> Rgba<u8> {
        Rgba([c.0, c.1, c.2, c.3])
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Color, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(ColorVisitor)
    }
}

/// Serde Serialization Support
struct ColorVisitor;

impl<'de> Visitor<'de> for ColorVisitor {
    type Value = Color;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a hex rgb or rgba color value")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let c = css_color::Rgba::from_str(value);
        if let Ok(c) = c {
            Ok(Color::from(c))
        } else {
            Err(de::Error::custom(""))
        }
    }
}

use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum AudioConversionError {
    IoError(std::io::Error),
    DecoderError(String),
    ResamplerError(String),
    UnsupportedFormat(String),
}

impl fmt::Display for AudioConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::DecoderError(e) => write!(f, "Decoder error: {}", e),
            Self::ResamplerError(e) => write!(f, "Resampler error: {}", e),
            Self::UnsupportedFormat(e) => write!(f, "Unsupported format: {}", e),
        }
    }
}

impl Error for AudioConversionError {}

impl From<std::io::Error> for AudioConversionError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err)
    }
}

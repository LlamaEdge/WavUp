use hound::{SampleFormat, WavSpec, WavWriter};
use rubato::{FftFixedInOut, Resampler};
use std::fs::File;
use std::io::BufWriter;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::codecs::{CODEC_TYPE_FLAC, CODEC_TYPE_OPUS, CODEC_TYPE_VORBIS};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::get_probe;

mod error;
pub use error::AudioConversionError;

pub struct AudioConverter {
    input_path: String,
    output_path: String,
    target_sample_rate: u32,
}

impl AudioConverter {
    pub fn new(input_path: String, output_path: String, target_sample_rate: u32) -> Self {
        Self {
            input_path,
            output_path,
            target_sample_rate,
        }
    }

    pub fn convert(&self) -> Result<(), AudioConversionError> {
        // Open input audio file
        let input_file = File::open(&self.input_path)?;
        let mss = MediaSourceStream::new(Box::new(input_file), Default::default());

        // Create probe hint
        let hint = Hint::new();

        // Use default format options
        let format_opts: FormatOptions = Default::default();
        let metadata_opts: MetadataOptions = Default::default();

        // Probe the format
        let probe = get_probe();
        let probed = probe
            .format(&hint, mss, &format_opts, &metadata_opts)
            .map_err(|e| AudioConversionError::DecoderError(e.to_string()))?;

        // Iterate through the tracks and find audio tracks.
        for track in probed.format.tracks() {
            println!("Found audio track:");
            let codec = track.codec_params.codec;
            match codec {
                CODEC_TYPE_VORBIS => println!("Codec: Vorbis"),
                CODEC_TYPE_OPUS => println!("Codec: Opus"),
                CODEC_TYPE_FLAC => println!("Codec: FLAC"),
                _ => println!("Codec: Other ({:?})", codec),
            }

            // Print additional codec parameters.
            if let Some(channels) = track.codec_params.channels {
                println!("Channels: {}", channels.count());
            }
            if let Some(sample_rate) = track.codec_params.sample_rate {
                println!("Sample rate: {} Hz", sample_rate);
            }
        }

        // Get the default track and decoder
        let track = probed
            .format
            .default_track()
            .ok_or_else(|| AudioConversionError::DecoderError("No default track found".into()))?;

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| AudioConversionError::DecoderError(e.to_string()))?;

        // Get sample rates and channels
        let input_sample_rate = track.codec_params.sample_rate.unwrap();
        let channels = track.codec_params.channels.unwrap().count() as u16;

        // Prepare WAV writer
        let spec = WavSpec {
            channels,
            sample_rate: self.target_sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };

        let output_file = File::create(&self.output_path)?;
        let mut writer = WavWriter::new(BufWriter::new(output_file), spec).map_err(|e| {
            AudioConversionError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e))
        })?;

        // Initialize resampler if needed
        let mut resampler = if input_sample_rate != self.target_sample_rate {
            Some(
                FftFixedInOut::<f32>::new(
                    input_sample_rate as usize,
                    self.target_sample_rate as usize,
                    4096,
                    channels as usize,
                )
                .map_err(|e| AudioConversionError::ResamplerError(e.to_string()))?,
            )
        } else {
            None
        };

        // Create buffers for resampling
        let mut input_buffer: Vec<Vec<f32>> = vec![Vec::new(); channels as usize];
        let mut accumulated_buffer: Vec<Vec<f32>> = vec![Vec::new(); channels as usize];

        // Process audio
        let mut format = probed.format;
        while let Ok(packet) = format.next_packet() {
            let decoded = decoder
                .decode(&packet)
                .map_err(|e| AudioConversionError::DecoderError(e.to_string()))?;

            match decoded {
                AudioBufferRef::F32(buffer) => {
                    self.process_audio_buffer(
                        &buffer,
                        &mut writer,
                        &mut resampler,
                        &mut input_buffer,
                        &mut accumulated_buffer,
                    )?;
                }
                _ => {
                    return Err(AudioConversionError::UnsupportedFormat(
                        "Only F32 audio buffers are supported".into(),
                    ))
                }
            }
        }

        // Process remaining samples
        self.process_remaining_samples(
            &mut writer,
            &mut resampler,
            &mut input_buffer,
            &accumulated_buffer,
        )?;

        writer.finalize().map_err(|e| {
            AudioConversionError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e))
        })?;

        Ok(())
    }

    fn process_audio_buffer(
        &self,
        buffer: &symphonia::core::audio::AudioBuffer<f32>,
        writer: &mut WavWriter<BufWriter<File>>,
        resampler: &mut Option<FftFixedInOut<f32>>,
        input_buffer: &mut Vec<Vec<f32>>,
        accumulated_buffer: &mut Vec<Vec<f32>>,
    ) -> Result<(), AudioConversionError> {
        let channels = buffer.spec().channels.count();

        // Accumulate decoded audio data
        for ch in 0..channels {
            accumulated_buffer[ch].extend_from_slice(buffer.chan(ch));
        }

        if let Some(resampler) = resampler {
            let chunk_size = resampler.input_frames_next();

            // Process complete chunks
            while accumulated_buffer[0].len() >= chunk_size {
                // Prepare input chunk
                for ch in 0..channels {
                    input_buffer[ch] = accumulated_buffer[ch][..chunk_size].to_vec();
                    accumulated_buffer[ch].drain(0..chunk_size);
                }

                // Perform resampling
                let output_buffer = resampler
                    .process(&input_buffer, None)
                    .map_err(|e| AudioConversionError::ResamplerError(e.to_string()))?;

                // Write resampled audio
                self.write_output_buffer(writer, &output_buffer)?;
            }
        } else {
            // No resampling needed, write directly
            for frame in 0..buffer.frames() {
                for channel in 0..channels {
                    let sample = buffer.chan(channel)[frame];
                    let int_sample = (sample * i16::MAX as f32) as i16;
                    writer.write_sample(int_sample).map_err(|e| {
                        AudioConversionError::IoError(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            e,
                        ))
                    })?;
                }
            }
        }

        Ok(())
    }

    fn process_remaining_samples(
        &self,
        writer: &mut WavWriter<BufWriter<File>>,
        resampler: &mut Option<FftFixedInOut<f32>>,
        input_buffer: &mut Vec<Vec<f32>>,
        accumulated_buffer: &[Vec<f32>],
    ) -> Result<(), AudioConversionError> {
        if let Some(resampler) = resampler {
            if !accumulated_buffer[0].is_empty() {
                let chunk_size = resampler.input_frames_next();
                let channels = accumulated_buffer.len();

                // Pad remaining samples
                for ch in 0..channels {
                    input_buffer[ch] = accumulated_buffer[ch].clone();
                    input_buffer[ch].resize(chunk_size, 0.0);
                }

                // Final resampling
                let output_buffer = resampler
                    .process(&input_buffer, None)
                    .map_err(|e| AudioConversionError::ResamplerError(e.to_string()))?;

                self.write_output_buffer(writer, &output_buffer)?;
            }
        }

        Ok(())
    }

    fn write_output_buffer(
        &self,
        writer: &mut WavWriter<BufWriter<File>>,
        output_buffer: &[Vec<f32>],
    ) -> Result<(), AudioConversionError> {
        for frame in 0..output_buffer[0].len() {
            for channel in 0..output_buffer.len() {
                let sample = output_buffer[channel][frame];
                let int_sample = (sample * i16::MAX as f32) as i16;
                writer.write_sample(int_sample).map_err(|e| {
                    AudioConversionError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e))
                })?;
            }
        }
        Ok(())
    }
}

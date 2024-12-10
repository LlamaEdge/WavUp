#[cfg(feature = "logging")]
#[macro_use]
extern crate log;

mod error;
pub use error::AudioConversionError;

use hound::{WavSpec, WavWriter};
use rubato::{FftFixedInOut, Resampler};
use std::fs::File;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_FLAC, CODEC_TYPE_OPUS, CODEC_TYPE_VORBIS},
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

#[derive(Debug, Default)]
pub struct AudioConverterBuilder {
    input_path: String,
    output_path: String,
    target_sample_rate: u32,
}
impl AudioConverterBuilder {
    /// Create a new audio converter builder.
    ///
    /// # Arguments
    ///
    /// * `output_path` - The path to the output WAV file.
    ///
    /// * `target_sample_rate` - The target sample rate for the output WAV file.
    pub fn new(output_path: impl Into<String>, target_sample_rate: u32) -> Self {
        Self {
            output_path: output_path.into(),
            target_sample_rate,
            ..Default::default()
        }
    }

    /// Set the input path for the audio converter if the input is an audio file.
    ///
    /// # Arguments
    ///
    /// * `input_path` - The path to the input audio file.
    pub fn with_input_path<S: Into<String>>(mut self, input_path: S) -> Self {
        self.input_path = input_path.into();
        self
    }

    /// Build the audio converter.
    pub fn build(self) -> AudioConverter {
        AudioConverter {
            input_path: self.input_path,
            output_path: self.output_path,
            target_sample_rate: self.target_sample_rate,
        }
    }
}

#[derive(Debug)]
pub struct AudioConverter {
    input_path: String,
    output_path: String,
    target_sample_rate: u32,
}
impl AudioConverter {
    pub fn convert_audio(&self) -> Result<(), Box<dyn std::error::Error>> {
        let file = File::open(&self.input_path)?;
        let media_source = MediaSourceStream::new(Box::new(file), Default::default());
        self.convert_audio_internal(media_source)
    }

    pub fn convert_audio_from_bytes(&self, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let buffer = std::io::Cursor::new(bytes.to_vec());
        let media_source = MediaSourceStream::new(Box::new(buffer), Default::default());
        self.convert_audio_internal(media_source)
    }

    fn convert_audio_internal(
        &self,
        media_source: MediaSourceStream,
    ) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(feature = "logging")]
        info!(target: "stdout", "Probing audio");

        let mut hint = Hint::new();
        hint.with_extension("oga");

        let format_opts: FormatOptions = Default::default();
        let metadata_opts: MetadataOptions = Default::default();
        let decoder_opts: DecoderOptions = Default::default();

        // Probe the media source
        let probed = symphonia::default::get_probe().format(
            &hint,
            media_source,
            &format_opts,
            &metadata_opts,
        )?;
        let mut format = probed.format;

        #[cfg(feature = "logging")]
        {
            // Iterate through the tracks and find audio tracks.
            for track in format.tracks() {
                let codec = track.codec_params.codec;
                match codec {
                    CODEC_TYPE_VORBIS => {
                        info!(target: "stdout", "Codec of input audio: Vorbis");
                    }
                    CODEC_TYPE_OPUS => info!(target: "stdout", "Codec of input audio: Opus"),
                    CODEC_TYPE_FLAC => info!(target: "stdout", "Codec of input audio: FLAC"),
                    _ => info!(target: "stdout", "Codec of input audio: Other ({:?})", codec),
                }

                // Print additional codec parameters.
                if let Some(channels) = track.codec_params.channels {
                    info!(target: "stdout", "Channels of input audio: {}", channels.count());
                }
                if let Some(sample_rate) = track.codec_params.sample_rate {
                    info!(target: "stdout", "Sample rate of input audio: {} Hz", sample_rate);
                }
            }
        }

        let track = format.default_track().unwrap();
        let mut decoder =
            symphonia::default::get_codecs().make(&track.codec_params, &decoder_opts)?;

        // Get audio info
        let track_info = track.codec_params.clone();
        let channels = track_info.channels.unwrap().count();
        let original_sample_rate = track_info.sample_rate.unwrap();

        #[cfg(feature = "logging")]
        {
            debug!(target: "stdout", "channels: {}", channels);
            debug!(target: "stdout", "original_sample_rate: {}", original_sample_rate);
        }

        // Set up WAV writer
        let spec = WavSpec {
            channels: channels as u16,
            sample_rate: self.target_sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        #[cfg(feature = "logging")]
        info!(target: "stdout", "generated wav spec: {:?}", spec);

        // Create WAV writer
        let mut wav_writer = WavWriter::create(&self.output_path, spec)?;

        if original_sample_rate == self.target_sample_rate {
            // No resampling needed
            let all_samples = self.process_audio_samples(
                &mut *format,
                &mut *decoder,
                channels,
                original_sample_rate,
            )?;

            #[cfg(feature = "logging")]
            info!(target: "stdout", "Writing {} audio samples to WAV file: {}", all_samples.len(), &self.output_path);

            for sample in all_samples {
                wav_writer.write_sample((sample * 32768.0_f32) as i16)?;
            }
        } else {
            #[cfg(feature = "logging")]
            info!(
                target: "stdout",
                "Resampling from {}Hz to {}Hz",
                original_sample_rate, self.target_sample_rate
            );

            // Collect all samples
            let all_samples = self.process_audio_samples(
                &mut *format,
                &mut *decoder,
                channels,
                original_sample_rate,
            )?;

            #[cfg(feature = "logging")]
            info!(target: "stdout", "Resampling audio");

            // Prepare samples for resampler (separate channels)
            let mut input_channels: Vec<Vec<f32>> = vec![Vec::new(); channels];
            for (i, sample) in all_samples.iter().enumerate() {
                input_channels[i % channels].push(*sample);
            }

            // Create resampler
            let mut resampler = FftFixedInOut::<f32>::new(
                original_sample_rate as usize,
                self.target_sample_rate as usize,
                4096,
                channels,
            )?;

            // Process the audio in chunks
            let chunk_size = resampler.input_frames_next();
            let mut output_buffer = vec![Vec::new(); channels];

            // Process full chunks
            let mut pos = 0;
            while pos + chunk_size <= input_channels[0].len() {
                let mut chunk = vec![Vec::new(); channels];
                for ch in 0..channels {
                    chunk[ch] = input_channels[ch][pos..pos + chunk_size].to_vec();
                }

                if let Ok(mut resampled_chunk) = resampler.process(&chunk, None) {
                    for ch in 0..channels {
                        output_buffer[ch].append(&mut resampled_chunk[ch]);
                    }
                }
                pos += chunk_size;
            }

            // Process remaining samples if any
            if pos < input_channels[0].len() {
                let mut final_chunk = vec![Vec::new(); channels];
                for ch in 0..channels {
                    final_chunk[ch] = input_channels[ch][pos..].to_vec();
                    // Pad with zeros if necessary
                    final_chunk[ch].resize(chunk_size, 0.0);
                }

                if let Ok(resampled_chunk) = resampler.process(&final_chunk, None) {
                    let remaining_samples = (input_channels[0].len() - pos)
                        * self.target_sample_rate as usize
                        / original_sample_rate as usize;
                    for ch in 0..channels {
                        output_buffer[ch].extend(&resampled_chunk[ch][..remaining_samples]);
                    }
                }
            }

            #[cfg(feature = "logging")]
            info!(target: "stdout", "Writing resampled audio to WAV file: {}", &self.output_path);

            // Write resampled data
            for i in 0..output_buffer[0].len() {
                for item in output_buffer.iter().take(channels) {
                    let sample = (item[i] * 32768.0) as i16;
                    wav_writer.write_sample(sample)?;
                }
            }
        }

        #[cfg(feature = "logging")]
        info!(target: "stdout", "Finalizing WAV file");

        wav_writer.finalize()?;

        Ok(())
    }

    fn process_audio_samples(
        &self,
        format: &mut dyn symphonia::core::formats::FormatReader,
        decoder: &mut dyn symphonia::core::codecs::Decoder,
        channels: usize,
        original_sample_rate: u32,
    ) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        #[cfg(feature = "logging")]
        info!(target: "stdout", "Processing audio samples");

        #[cfg(feature = "logging")]
        debug!(
            target: "stdout",
            "channels: {}, original_sample_rate: {}",
            channels, original_sample_rate
        );

        let mut all_samples = Vec::new();
        let mut sample_buf: Option<SampleBuffer<f32>> = None;

        while let Ok(packet) = format.next_packet() {
            let decoded = decoder.decode(&packet)?;
            if sample_buf.is_none() {
                sample_buf = Some(SampleBuffer::new(
                    decoded.capacity() as u64,
                    *decoded.spec(),
                ));
            }
            let sample_buf = sample_buf.as_mut().unwrap();
            sample_buf.copy_interleaved_ref(decoded);

            all_samples.extend(sample_buf.samples().iter().copied());
        }

        self.trim_ending_silence(&all_samples, channels, original_sample_rate)
    }

    fn trim_ending_silence(
        &self,
        samples: &[f32],
        channels: usize,
        sample_rate: u32,
    ) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        #[cfg(feature = "logging")]
        info!(target: "stdout", "Trimming ending silence");
        // -20 dB ≈ 0.1
        // -30 dB ≈ 0.0316
        // -40 dB ≈ 0.01
        // -50 dB ≈ 0.0032
        // -60 dB ≈ 0.001
        let threshold = 0.01;

        // Look for the last non-silent sample
        let mut last_non_silent_index = 0;

        #[cfg(feature = "logging")]
        debug!(
            target: "stdout",
            "len of samples: {}, channels: {}, sample_rate: {}",
            samples.len(),
            channels,
            sample_rate
        );

        if samples.len() % channels != 0 {
            let err_msg = format!(
                "The number of samples is not divisible by the number of channels. samples.len(): {}, channels: {}",
                samples.len(),
                channels
            );

            error!(target: "stdout", "{}", err_msg);

            return Err(AudioConversionError::InvalidSampleCount(err_msg).into());
        }

        // First pass: find the last non-silent sample
        let num_samples = samples.len() / channels;
        for i in (0..num_samples).rev().step_by(channels) {
            let mut silent = true;
            for ch in 0..channels {
                if !self.is_silent(samples[i + ch], threshold) {
                    silent = false;
                    last_non_silent_index = i;
                    break;
                }
            }
            if !silent {
                break;
            }
        }

        // Add a small buffer (e.g., 0.5 seconds) after the last non-silent sample
        let buffer_duration_secs = 0.5;
        let buffer_samples = (buffer_duration_secs * sample_rate as f32) as usize * channels;
        let trim_index = (last_non_silent_index + channels - 1 + buffer_samples).min(samples.len());

        Ok(samples[..trim_index].to_vec())
    }

    fn is_silent(&self, sample: f32, threshold: f32) -> bool {
        sample.abs() < threshold
    }
}

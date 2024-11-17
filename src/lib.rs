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

    pub fn convert_audio(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 打开OGA文件
        let file = File::open(&self.input_path)?;
        let media_source = MediaSourceStream::new(Box::new(file), Default::default());

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

        {
            // Iterate through the tracks and find audio tracks.
            for track in format.tracks() {
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
        }

        let track = format.default_track().unwrap();
        let mut decoder =
            symphonia::default::get_codecs().make(&track.codec_params, &decoder_opts)?;

        // Get audio info
        let track_info = track.codec_params.clone();
        let channels = track_info.channels.unwrap().count();
        let original_sample_rate = track_info.sample_rate.unwrap();

        // Set up WAV writer
        let spec = WavSpec {
            channels: channels as u16,
            sample_rate: self.target_sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut wav_writer = WavWriter::create(&self.output_path, spec)?;

        if original_sample_rate == self.target_sample_rate {
            // No resampling needed
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

                for sample in sample_buf.samples() {
                    wav_writer.write_sample((sample * 32768.0_f32) as i16)?;
                }
            }
        } else {
            println!(
                "Resampling from {}Hz to {}Hz",
                original_sample_rate, self.target_sample_rate
            );

            // Collect all samples first
            let mut all_samples = Vec::new();
            let mut sample_buf = None;

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

                all_samples.extend(sample_buf.samples().iter().map(|s: &f32| *s as f32));
            }

            // Convert samples to f32
            let samples_f32: Vec<f32> = all_samples.iter().map(|s| *s as f32).collect();

            // Prepare samples for resampler (separate channels)
            let mut input_channels: Vec<Vec<f32>> = vec![Vec::new(); channels];
            for (i, sample) in samples_f32.iter().enumerate() {
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

            // Write resampled data
            for i in 0..output_buffer[0].len() {
                for ch in 0..channels {
                    let sample = (output_buffer[ch][i] * 32768.0) as i16;
                    wav_writer.write_sample(sample)?;
                }
            }
        }

        wav_writer.finalize()?;

        Ok(())
    }
}

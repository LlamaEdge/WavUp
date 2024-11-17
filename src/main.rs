use clap::Parser;
use hound::SampleFormat;
use hound::{WavSpec, WavWriter};
use rubato::{FftFixedInOut, Resampler};
use std::fs::File;
use std::io::BufWriter;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::get_probe;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input audio file path
    #[arg(short, long)]
    input: String,

    /// Output WAV file path
    #[arg(short, long, default_value = "output.wav")]
    out_file: String,

    /// Output sample rate in Hz
    #[arg(short = 'r', long, default_value_t = 44100)]
    sample_rate: u32,
}

fn main() {
    let args = Args::parse();
    convert_to_wav(&args.input, &args.out_file, args.sample_rate);
}

fn convert_to_wav(input_path: &str, output_path: &str, sample_rate: u32) {
    // 打开输入音频文件
    let input_file = File::open(input_path).expect("Failed to open input file");
    let mss = MediaSourceStream::new(Box::new(input_file), Default::default());

    // 创建探测器提示
    let hint = Hint::new();

    // 使用默认配置探测格式
    let format_opts: FormatOptions = Default::default();
    let metadata_opts: MetadataOptions = Default::default();

    // 探测音频格式
    let probe = get_probe();
    let probed = probe
        .format(&hint, mss, &format_opts, &metadata_opts)
        .expect("Failed to decode audio format");

    // 获取默认轨道和解码器
    let track = probed
        .format
        .default_track()
        .expect("No default track found");
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .expect("Failed to create decoder");

    // 准备 WAV 文件写入器
    let spec = WavSpec {
        channels: track.codec_params.channels.unwrap().count() as u16,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let output_file = File::create(output_path).expect("Failed to create output WAV file");
    let mut writer =
        WavWriter::new(BufWriter::new(output_file), spec).expect("Failed to initialize WAV writer");

    // Get the input and output sample rates
    let input_sample_rate = track.codec_params.sample_rate.unwrap();
    let output_sample_rate = sample_rate;

    // Initialize the resampler if needed
    let channels = spec.channels as usize;
    let mut resampler = if input_sample_rate != output_sample_rate {
        Some(
            FftFixedInOut::<f32>::new(
                input_sample_rate as usize,
                output_sample_rate as usize,
                4096, // Smaller buffer size for better chunk handling
                channels,
            )
            .expect("Failed to create resampler"),
        )
    } else {
        None
    };

    // Create buffers for resampling
    let mut input_buffer: Vec<Vec<f32>> = vec![Vec::new(); channels];
    let mut accumulated_buffer: Vec<Vec<f32>> = vec![Vec::new(); channels];

    // Decode audio and write to WAV file
    let mut format = probed.format;
    while let Ok(packet) = format.next_packet() {
        if let Ok(decoded) = decoder.decode(&packet) {
            if let AudioBufferRef::F32(buffer) = decoded {
                // Accumulate decoded audio data
                for ch in 0..channels {
                    accumulated_buffer[ch].extend_from_slice(buffer.chan(ch));
                }

                if let Some(resampler) = &mut resampler {
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
                            .expect("Failed to resample audio");

                        // Write resampled audio
                        for frame in 0..output_buffer[0].len() {
                            for channel in 0..channels {
                                let sample = output_buffer[channel][frame];
                                let int_sample = (sample * i16::MAX as f32) as i16;
                                writer.write_sample(int_sample).unwrap();
                            }
                        }
                    }
                } else {
                    // No resampling needed, write directly
                    for frame in 0..buffer.frames() {
                        for channel in 0..channels {
                            let sample = buffer.chan(channel)[frame];
                            let int_sample = (sample * i16::MAX as f32) as i16;
                            writer.write_sample(int_sample).unwrap();
                        }
                    }
                }
            } else {
                eprintln!("Unsupported audio buffer type");
            }
        }
    }

    // Process any remaining samples if resampling
    if let Some(resampler) = &mut resampler {
        if !accumulated_buffer[0].is_empty() {
            let chunk_size = resampler.input_frames_next();

            // Pad the remaining samples with silence if needed
            for ch in 0..channels {
                let current_len = accumulated_buffer[ch].len();
                if current_len < chunk_size {
                    accumulated_buffer[ch].resize(chunk_size, 0.0);
                }
                input_buffer[ch] = accumulated_buffer[ch].clone();
            }

            // Perform final resampling
            let output_buffer = resampler
                .process(&input_buffer, None)
                .expect("Failed to resample audio");

            // Write final resampled audio
            for frame in 0..output_buffer[0].len() {
                for channel in 0..channels {
                    let sample = output_buffer[channel][frame];
                    let int_sample = (sample * i16::MAX as f32) as i16;
                    writer.write_sample(int_sample).unwrap();
                }
            }
        }
    }

    writer.finalize().expect("Failed to finalize WAV file");
    println!(
        "Converted {} to {} (sample rate: {} Hz)",
        input_path, output_path, output_sample_rate
    );
}

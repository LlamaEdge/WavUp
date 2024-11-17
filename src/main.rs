use hound::SampleFormat;
use hound::{WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::get_probe;

fn main() {
    let input_path = "audio/example.ogg"; // 替换为你的输入文件路径
    let output_path = "output.wav"; // 替换为你的输出文件路径
    convert_to_wav(input_path, output_path);
}

fn convert_to_wav(input_path: &str, output_path: &str) {
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
        sample_rate: track.codec_params.sample_rate.unwrap(),
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let output_file = File::create(output_path).expect("Failed to create output WAV file");
    let mut writer =
        WavWriter::new(BufWriter::new(output_file), spec).expect("Failed to initialize WAV writer");

    // 解码音频并写入 WAV 文件
    let mut format = probed.format;
    while let Ok(packet) = format.next_packet() {
        if let Ok(decoded) = decoder.decode(&packet) {
            if let AudioBufferRef::F32(buffer) = decoded {
                // 获取声道数
                let channels = buffer.spec().channels.count();

                // 遍历所有采样点
                for frame in 0..buffer.frames() {
                    // 遍历所有声道
                    for channel in 0..channels {
                        let sample = buffer.chan(channel)[frame];
                        let int_sample = (sample * i16::MAX as f32) as i16;
                        writer.write_sample(int_sample).unwrap();
                    }
                }
            } else {
                eprintln!("Unsupported audio buffer type");
            }
        }
    }

    writer.finalize().expect("Failed to finalize WAV file");
    println!("Converted {} to {}", input_path, output_path);
}

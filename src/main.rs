use clap::Parser;
use std::process;

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

    let converter = wavup::AudioConverter::new(args.input, args.out_file.clone(), args.sample_rate);

    if let Err(e) = converter.convert_audio() {
        eprintln!("Error converting audio: {}", e);
        process::exit(1);
    }

    println!(
        "Successfully converted audio to {} (sample rate: {} Hz)",
        args.out_file, args.sample_rate
    );
}

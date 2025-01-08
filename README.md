# WavUp

`Wavup` is a simple tool to convert audio files to WAV format. The audio formats supported are:

- The formats supported are `caf`, `isomp4`, `mkv`, `ogg`, `aiff`, `wav`.

- The codecs supported are `aac`, `adpcm`, `alac`, `flac`, `mp1`, `mp2`, `mp3`, `pcm`, `vorbis`.

## Usage

- Use as a wasm app

  ```bash
  wasmedge --dir .:. ./target/wasm32-wasip1/release/wavup.wasm \
    --input ./audio/mono_ch_audio.mp3 \
    --out-file output.wav

  # or using short forms:
  wasmedge --dir .:. ./target/wasm32-wasip1/release/wavup.wasm -i audio/mono_ch_audio.mp3 -o output.wav
  ```

- Use as a library

  Add the following to your `Cargo.toml`:

    ```toml
    [dependencies]
    wavup = "0.1.0"
    ```

  You can find the API reference [here](https://docs.rs/wavup/latest/wavup/).

# WavUp

This is a simple tool to convert audio files to WAV format.

- The formats supported are `caf`, `isomp4`, `mkv`, `ogg`, `aiff`, `wav`.

- The codecs supported are `aac`, `adpcm`, `alac`, `flac`, `mp1`, `mp2`, `mp3`, `pcm`, `vorbis`.

## Usage

```bash
wasmedge --dir .:. ./target/wasm32-wasip1/release/wavup.wasm \
  --input ./audio/example.ogg \
  --out-file output.wav

# or using short forms:
wasmedge --dir .:. ./target/wasm32-wasip1/release/wavup.wasm -i audio/example.ogg -o output.wav
```

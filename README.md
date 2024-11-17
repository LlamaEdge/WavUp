# TooWav

This is a simple tool to convert audio files to WAV format.

```bash
wasmedge --dir .:. ./target/wasm32-wasip1/release/wavup.wasm --input ./audio/example.ogg --out-file output.wav
# or using short forms:
wasmedge --dir .:. ./target/wasm32-wasip1/release/wavup.wasm -i audio/example.ogg -o output.wav
```

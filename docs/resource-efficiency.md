# Resource efficiency (0.26.4)

All application changes are Rust. The selected transcription model, quantization,
language detection, Whisper decoding parameters and audio filter remain unchanged.

## Audio

The normal Symphonia/Opus waveform path processes decoder packets instead of
retaining the complete PCM recording. A container frame-count hint avoids a second
decode when accurate. Missing hints use a counting pass; inaccurate hints are
checked against the actual decoded length and retried with the exact count.
Even recordings shorter than the 160 waveform columns retain the old RMS values.
AVI and platform-only codecs keep the original fallback decoder.

Plain WAV output writes decoded packets directly, with identical 16-bit samples.
Conversions requiring normalization, channel changes or trimming keep their existing
processing path; MP3/FLAC/Opus conversion has not been rewritten in this change.

The transcription decoder downmixes packets and incrementally applies the same
256-phase Hann-windowed sinc filter. It retains source samples only around the
filter overlap; the final 16 kHz mono array remains necessary for Whisper.
It does not split inference into independent audio chunks or alter context.

## Shared heavy-job admission

The wheel, main panel, compact panel and CLI conversions share a process-wide,
reentrant RAII gate with transcription. Recursive archive conversions do not
deadlock. Waiting transcription/panel jobs can be cancelled. This is deliberately
a per-process limit, not a machine-wide service or inter-process lock. Preview
loading and downloads are not serialized by this gate.

## GPU memory headroom

The existing Vulkan/iGPU path remains enabled. Before loading the engine, current
free physical RAM is compared with a conservative estimate of three times model
file size plus 512 MiB. This is a heuristic for additional headroom, **not** a
measurement of model RAM or GPU memory, and does not claim to select the fastest
backend. Unknown memory availability preserves the existing GPU behavior.

Under low RAM the CPU is tried with the **same model**. If a running GPU engine
dies, the current file is retried once on CPU without changing precision. Existing
CPU instruction selection and hardware video encoding are retained.

## Verification and measurements

Windows x64 release library tests and CachyOS release library tests cover packet
waveform equivalence, WAV sample identity, compressed codecs, cancellation and
mutual exclusion across concurrent workers. Standalone resampler tests compare
bit-for-bit with the frozen previous algorithm at 8, 16, 22.05, 44.1, 48 and 96 kHz,
including single-sample packets and boundaries. Passband/alias suppression tests
remain in place.

Local Windows x64 measurements, separate processes, sampled peak process working
set (includes harness overhead; not a whole-system measurement):

| Workload | Buffered reference | Packet implementation |
|---|---:|---:|
| Waveform, 20 minute 48 kHz stereo WAV | 307.33 MiB | 19.33 MiB |
| Resampling, 10 minute 48 kHz mono to 16 kHz | 158.84 MiB | 76.56 MiB |

The resampler output contains 9,600,000 samples in both cases, with the same
rolling sample-bit hash `2064603a153458b4`. In this run the filter took about
0.33 s buffered and 0.51 s streaming. Waveform computation took about 0.50 s and
0.62 s respectively. These are memory optimizations, not a promise of universally
faster execution. Source-rate memory is bounded; output buffers and decoder
internals still consume memory. Measurements are representative, not hardware
requirements or guaranteed savings for every codec.

A Ryzen 7 5700U on CachyOS exposed its integrated AMD Radeon through Vulkan.
The candidate Vulkan engine and CPU mode transcribed the same synthetic Italian
sentence into identical text with `small-q8_0`; the GPU engine explicitly reported
`AMD Radeon Graphics (RADV RENOIR)` as active. This is a functional smoke test,
not a transcription accuracy benchmark.

### Reproduction

```powershell
cargo test --release --lib
rustc --edition 2021 --test trascrivi/src/resample.rs -O -o resample-tests.exe
./resample-tests.exe
./resample-tests.exe resample_memory_benchmark --ignored --nocapture
# Set CEDRO_BENCH_BUFFERED=1 to run the frozen buffered resampling reference.
```

The ignored library test `audio::streaming_tests::waveform_memory_benchmark`
accepts a WAV path in `CEDRO_BENCH_AUDIO`; `CEDRO_BENCH_BUFFERED=1` selects the
buffered reference. Run each mode in a fresh process when comparing peak memory.
Build dependencies and platform instructions remain those in the repository.

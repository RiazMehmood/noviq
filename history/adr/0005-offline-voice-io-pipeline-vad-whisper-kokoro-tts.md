# ADR-0005: Offline Voice I/O Pipeline — VAD + Whisper Wake Word + Kokoro TTS

- **Status:** Accepted
- **Date:** 2026-04-19
- **Feature:** 002-agent-task-overlay
- **Context:** App 2 is a hands-free desktop overlay. Users must be able to activate it
  by voice ("EchoAI") and receive spoken responses — all without network access. Two
  sub-decisions form an inseparable pipeline: (1) how to detect the custom wake phrase
  offline with sub-1-second latency, and (2) which TTS engine produces the spoken response.
  They are clustered because both are voice I/O, share the `ort` ONNX Runtime dependency,
  and would change together if the voice stack is upgraded. The wake-word approach also
  directly constrains which STT model is used for full-utterance transcription.

## Decision

**Continuous VAD (Silero) + whisper-rs wake-word buffer + Kokoro TTS for all voice I/O.**

Components:
- **Microphone capture** (`voice/mic_capture.rs`): `cpal` 0.15 continuous ring buffer at
  16 kHz mono. Low CPU overhead; stops when voice mode disabled.
- **Silero VAD** (`voice/vad.rs`): `silero_vad.onnx` (1.8 MB, Apache 2.0) loaded via `ort`.
  Runs inference per 512-sample frame (~32 ms). Detects speech onset with ≥ 95% precision.
  Triggers buffering only when speech is detected — avoids continuous whisper runs.
- **Wake-word pipeline** (`voice/wake_word.rs`): On speech onset, buffers a 3-second
  audio window → runs `whisper-rs` transcription on the buffer (~400–700 ms on Tier 2) →
  Jaro-Winkler fuzzy match (threshold ≥ 0.85, via `strsim`) against configured wake phrase
  (default "EchoAI", fully configurable in Settings).
- **Full-utterance STT** (`voice/stt.rs`): Post-wake, whisper-rs transcribes the user's
  complete instruction. Same model, no additional download.
- **Kokoro TTS** (`voice/tts.rs`): `kokoro-v0_19.onnx` (83 MB, Apache 2.0) loaded via `ort`.
  Synthesises 24 kHz mono PCM. Playback via `rodio`. Evicted from RAM when not speaking
  (unlike the LLM orchestrator which stays warm).
- **ONNX Runtime reuse**: Both Silero VAD and Kokoro TTS use `ort` v2 — the same ONNX
  Runtime that `fastembed-rs` already links transitively. No additional dynamic library.

## Consequences

### Positive

- **Fully offline**: No audio leaves the device. Zero cloud API calls. Complies with
  Constitution Principle I unconditionally.
- **Custom wake phrase**: Any word or phrase can be set in Settings — no SDK constraint
  to pre-approved wake words.
- **ONNX reuse**: `ort` is already a transitive dependency. Adding Silero VAD and Kokoro
  TTS requires only ONNX model downloads, no new link-time dependencies.
- **VAD gating**: Silero VAD prevents continuous whisper runs — CPU usage near zero during
  silence, ~5 ms per 32 ms frame during speech only.
- **Latency within spec**: Wake detection < 1s (SC-009); TTS response start < 2s (SC-011).

### Negative

- **~1% false positive rate**: Similar-sounding phrases may trigger the wake pipeline.
  Mitigated by the grace-period cancel indicator shown in the overlay before transcription
  begins (edge case in spec).
- **Kokoro model download**: 83 MB model not bundled in installer — requires a one-time
  download by the user (same pattern as LLM weights). Voice features degrade gracefully
  with a clear message if the model is absent.
- **Whisper-rs / llama-cpp-2 ggml symbol conflict**: Known link conflict when both crates
  are compiled together (both bundle ggml). Existing workaround: `stt` feature flag is off
  by default; must be resolved before voice features ship (see Cargo.toml features comment).
- **Tier 1 hardware**: Wake detection on < 8 GB RAM machines may exceed 1s. Hardware-adaptive
  degradation should disable wake word and fall back to shortcut + manual mic button.

## Alternatives Considered

### Alternative A: Porcupine Wake Word SDK

Picovoice Porcupine with a custom "EchoAI" wake word model.

**Why rejected**: Custom wake words require a paid Picovoice account and cloud model training.
Violates Constitution Principle I. Free tier only supports pre-built phrases (Hey Siri, Hey
Google, etc.) — "EchoAI" is not available.

### Alternative B: Vosk Small Offline Model

40 MB offline ASR model with always-on streaming transcription for wake detection.

**Why rejected**: Lower accuracy than whisper-rs on short custom phrases; separate ONNX
toolchain separate from the existing `ort` dependency; 40 MB model vs 1.8 MB Silero VAD.
Vosk's streaming mode also burns more CPU than VAD-gated whisper buffering.

### Alternative C: System TTS (espeak / speech-dispatcher)

Use the OS text-to-speech daemon (`org.freedesktop.speech-dispatcher`) instead of Kokoro.

**Why rejected**: Constitution Technology Stack Invariants lock TTS to Kokoro TTS (local
neural, no cloud). espeak produces robotic quality that degrades UX significantly.
speech-dispatcher on some Linux configurations proxies to cloud TTS without user knowledge.

### Alternative D: Always-on Full Whisper Streaming

Run whisper-rs continuously on all audio instead of VAD-gated buffering.

**Why rejected**: Whisper on continuous audio at 16 kHz burns ~20–40% CPU continuously on
Tier 2 hardware — unacceptable for a "light and easy to use" overlay app.

## References

- Feature Spec: `specs/002-agent-task-overlay/spec.md` (FR-022–FR-026, US5, SC-009–SC-012)
- Implementation Plan: `specs/002-agent-task-overlay/plan.md` (AD-003, AD-004, Phase 2)
- Research: `specs/002-agent-task-overlay/research.md` (R-003, R-004)
- Related ADRs: ADR-0001 (shared ModelSlot — whisper-rs shares the ggml runtime with llama-cpp-2)
- Constitution: `.specify/memory/constitution.md` (Principle I: Privacy-First, TTS: Kokoro TTS lock)

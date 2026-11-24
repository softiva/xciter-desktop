//! Decodes songs ahead of time, runs them through levelling, crossfade, the effects and the equalizer on
//! its own thread, and feeds the sound card. Everything runs at the output's own rate, so one song runs
//! straight into the next with no gap.
use crate::dither::Dither;
use crate::dsp::{
    EffectSettings, EffectsChain, GraphicEq, OriginalMatch, OutputStage, SpectrumMeter, BANDS,
};
use crate::library::ReplayGain;
use crate::sync::lock;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat};
use rtrb::{Consumer, Producer, RingBuffer};
use rubato::{FftFixedIn, Resampler};
use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

mod deck;
mod handle;
mod model;
mod output;
mod renderer;

// Crate-internal items shared by the submodules.
use deck::*;
use output::*;
use renderer::*;

pub use handle::*;
pub use model::*;

#[cfg(test)]
mod tests;

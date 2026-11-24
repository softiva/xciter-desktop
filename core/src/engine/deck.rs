//! One song being decoded and resampled to the output rate.

use super::*;

/// One song being decoded.
pub(super) struct Deck {
    pub(super) item: Item,
    pub(super) format: Box<dyn FormatReader>,
    pub(super) decoder: Box<dyn Decoder>,
    pub(super) track: u32,
    pub(super) resampler: Option<FftFixedIn<f32>>,
    pub(super) input: [Vec<f32>; 2],
    pub(super) output: Vec<Vec<f32>>,
    pub(super) pending: Vec<f32>,
    pub(super) at: usize,
    pub(super) gain: f32,
    pub(super) total: i64,
    pub(super) read: i64,
    pub(super) sample_buffer: Option<SampleBuffer<f32>>,
    pub(super) input_ended: bool,
}

impl Deck {
    pub(super) fn open(item: &Item, seconds: f64, rate: u32, gain: f32) -> Option<Self> {
        let file = File::open(&item.path).ok()?;
        let stream = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = item.path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(extension);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                stream,
                &FormatOptions {
                    enable_gapless: true,
                    ..Default::default()
                },
                &MetadataOptions::default(),
            )
            .ok()?;
        let mut format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)?;
        let id = track.id;
        let source_rate = track.codec_params.sample_rate?;
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .ok()?;
        if seconds > 0.0 {
            format
                .seek(
                    SeekMode::Accurate,
                    SeekTo::Time {
                        time: Time::from(seconds),
                        track_id: Some(id),
                    },
                )
                .ok()?;
            decoder.reset();
        }
        let resampler = if source_rate == rate {
            None
        } else {
            FftFixedIn::<f32>::new(source_rate as usize, rate as usize, 1024, 2, 2).ok()
        };
        if source_rate != rate && resampler.is_none() {
            return None;
        }
        let output = resampler
            .as_ref()
            .map(|r: &FftFixedIn<f32>| r.output_buffer_allocate(true))
            .unwrap_or_default();
        Some(Self {
            item: item.clone(),
            format,
            decoder,
            track: id,
            resampler,
            input: [Vec::new(), Vec::new()],
            output,
            pending: Vec::new(),
            at: 0,
            gain,
            total: (item.duration * rate as f64) as i64,
            read: (seconds * rate as f64) as i64,
            sample_buffer: None,
            input_ended: false,
        })
    }

    pub(super) fn available(&self) -> usize {
        (self.pending.len() - self.at) / 2
    }

    pub(super) fn emit(&mut self, frames: usize) {
        for i in 0..frames {
            self.pending.push(self.output[0][i]);
            self.pending.push(self.output[1][i]);
        }
    }

    /// The next decoded chunk. False at the end of the song.
    pub(super) fn refill(&mut self) -> bool {
        self.pending.clear();
        self.at = 0;
        while self.pending.is_empty() {
            if self.input_ended {
                let leftover = self.input[0].len();
                let Some(resampler) = self.resampler.as_mut() else {
                    return false;
                };
                if leftover == 0 {
                    return false;
                }
                let Ok((_, written)) = resampler.process_partial_into_buffer(
                    Some(&self.input),
                    &mut self.output,
                    None,
                ) else {
                    return false;
                };
                self.input[0].clear();
                self.input[1].clear();
                self.emit(written);
                return written > 0;
            }
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(_) => {
                    self.input_ended = true;
                    continue;
                }
            };
            if packet.track_id() != self.track {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(DecodeError::DecodeError(_)) => continue,
                Err(_) => {
                    self.input_ended = true;
                    continue;
                }
            };
            let spec = *decoded.spec();
            let channels = spec.channels.count().max(1);
            let capacity = decoded.capacity() as u64;
            let buffer = match &mut self.sample_buffer {
                Some(buffer) if buffer.capacity() >= capacity as usize * channels => buffer,
                slot => slot.insert(SampleBuffer::<f32>::new(capacity, spec)),
            };
            buffer.copy_interleaved_ref(decoded);
            if self.resampler.is_none() {
                for frame in buffer.samples().chunks(channels) {
                    self.pending.push(frame[0]);
                    self.pending
                        .push(if channels > 1 { frame[1] } else { frame[0] });
                }
                continue;
            }
            for frame in buffer.samples().chunks(channels) {
                self.input[0].push(frame[0]);
                self.input[1].push(if channels > 1 { frame[1] } else { frame[0] });
            }
            while let Some(resampler) = self.resampler.as_mut() {
                if self.input[0].len() < resampler.input_frames_next() {
                    break;
                }
                let Ok((consumed, written)) =
                    resampler.process_into_buffer(&self.input, &mut self.output, None)
                else {
                    return false;
                };
                self.input[0].drain(..consumed);
                self.input[1].drain(..consumed);
                self.emit(written);
            }
        }
        true
    }
}

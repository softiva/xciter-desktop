//! The render thread: follows commands, runs the decks, effects, EQ and crossfades, and fills the ring.

use super::*;

pub(super) struct Renderer {
    pub(super) shared: Arc<Shared>,
    pub(super) producer: Producer<f32>,
    pub(super) commands: Receiver<Command>,
    pub(super) events: Sender<Event>,
    pub(super) plan: Plan,
    pub(super) position: usize,
    pub(super) deck: Option<Deck>,
    pub(super) outgoing: Option<Deck>,
    pub(super) prepared: Option<Deck>,
    pub(super) failed_next: Option<String>,
    pub(super) fade_total: usize,
    pub(super) fade_done: usize,
    pub(super) ended: bool,
    pub(super) finished_sent: bool,
    pub(super) wanted_playing: bool,
    pub(super) scheduled: i64,
    pub(super) notified: Option<String>,
    pub(super) applied: SoundSnapshot,
    pub(super) rate: u32,
    pub(super) generation: u64,
    pub(super) chain: EffectsChain,
    pub(super) eq: GraphicEq,
    pub(super) stage: OutputStage,
    pub(super) meter: SpectrumMeter,
    pub(super) left: Vec<f32>,
    pub(super) right: Vec<f32>,
    pub(super) dry_left: Vec<f32>,
    pub(super) dry_right: Vec<f32>,
    pub(super) matcher: OriginalMatch,
}

impl Renderer {
    pub(super) fn new(
        shared: Arc<Shared>,
        producer: Producer<f32>,
        commands: Receiver<Command>,
        events: Sender<Event>,
    ) -> Self {
        let rate = 48000.0;
        Self {
            shared,
            producer,
            commands,
            events,
            plan: Plan::default(),
            position: 0,
            deck: None,
            outgoing: None,
            prepared: None,
            failed_next: None,
            fade_total: 0,
            fade_done: 0,
            ended: false,
            finished_sent: true,
            wanted_playing: false,
            scheduled: 0,
            notified: None,
            applied: SoundSnapshot::default(),
            rate: 0,
            generation: 0,
            chain: EffectsChain::new(rate),
            eq: GraphicEq::new(rate),
            stage: OutputStage::new(rate),
            meter: SpectrumMeter::new(rate),
            left: vec![0.0; BLOCK],
            right: vec![0.0; BLOCK],
            dry_left: vec![0.0; BLOCK],
            dry_right: vec![0.0; BLOCK],
            matcher: OriginalMatch::with_delay(rate, 0.010),
        }
    }

    pub(super) fn run(mut self) {
        loop {
            while let Ok(command) = self.commands.try_recv() {
                self.handle(command);
            }
            self.follow_device();
            self.check_boundary();
            let room = self.producer.slots() / 2;
            let target =
                RING_FRAMES.saturating_sub((self.rate as usize * 3 / 10).min(RING_FRAMES - BLOCK));
            if self.deck.is_none() || room <= target + BLOCK {
                self.check_finished();
                match self.commands.recv_timeout(Duration::from_millis(8)) {
                    Ok(command) => self.handle(command),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                continue;
            }
            self.render_block();
        }
    }

    pub(super) fn sound(&self) -> SoundSnapshot {
        lock(&self.shared.sound).clone()
    }

    pub(super) fn follow_device(&mut self) {
        let generation = self.shared.device_generation.load(Ordering::Acquire);
        if generation == self.generation {
            return;
        }
        self.generation = generation;
        let rate = self.shared.rate.load(Ordering::Acquire);
        if rate == self.rate {
            return;
        }
        // The sound is rebuilt for the new output, carrying on from where the listener is.
        let place = lock(&self.shared.last).clone();
        let seconds = place.map(|p| p.1).unwrap_or(0.0);
        self.rate = rate;
        let r = rate as f64;
        self.chain = EffectsChain::new(r);
        self.eq = GraphicEq::new(r);
        self.stage = OutputStage::new(r);
        self.meter = SpectrumMeter::new(r);
        self.matcher = OriginalMatch::with_delay(r, 0.010);
        self.applied = SoundSnapshot::default();
        if self.deck.is_some() {
            let (plan, position, playing) = (self.plan.clone(), self.position, self.wanted_playing);
            self.begin(plan, position, seconds, playing);
        }
    }

    pub(super) fn flush(&mut self) {
        self.shared.flush.store(true, Ordering::Release);
        for _ in 0..200 {
            if !self.shared.flush.load(Ordering::Acquire) {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        self.shared.flush.store(false, Ordering::Release);
        self.shared.played.store(0, Ordering::Relaxed);
    }

    pub(super) fn begin(&mut self, plan: Plan, position: usize, seconds: f64, playing: bool) {
        self.flush();
        self.plan = plan;
        self.position = position;
        self.outgoing = None;
        self.prepared = None;
        self.failed_next = None;
        self.fade_total = 0;
        self.fade_done = 0;
        self.ended = false;
        self.finished_sent = false;
        self.scheduled = 0;
        self.notified = None;
        self.chain.reset();
        self.matcher.reset();
        self.eq.reset();
        self.stage.reset();
        self.meter.reset();
        lock(&self.shared.marks).clear();
        lock(&self.shared.bars).clear();
        self.wanted_playing = playing;
        self.shared.paused.store(!playing, Ordering::Relaxed);
        self.deck = None;
        self.open(position, seconds, 0);
    }

    pub(super) fn handle(&mut self, command: Command) {
        match command {
            Command::Start {
                plan,
                position,
                seconds,
                playing,
            } => {
                if self.rate == 0 {
                    for _ in 0..500 {
                        if self.shared.rate.load(Ordering::Acquire) != 0 {
                            break;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    self.follow_device();
                }
                self.begin(plan, position, seconds, playing);
            }
            Command::Update(plan) => {
                let id = self.deck.as_ref().map(|d| d.item.id.clone());
                self.plan = plan;
                if let Some(id) = id {
                    if let Some(found) = self.plan.items.iter().position(|i| i.id == id) {
                        self.position = found;
                    }
                }
                let next_id = self
                    .plan
                    .next(self.position)
                    .and_then(|n| self.plan.items.get(n))
                    .map(|i| i.id.clone());
                if self
                    .prepared
                    .as_ref()
                    .map(|p| Some(p.item.id.clone()) != next_id)
                    .unwrap_or(false)
                {
                    self.prepared = None;
                }
            }
            Command::Play => {
                self.wanted_playing = true;
                self.shared.paused.store(false, Ordering::Relaxed);
            }
            Command::Pause => {
                self.wanted_playing = false;
                self.shared.paused.store(true, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn open(&mut self, position: usize, seconds: f64, offset: usize) {
        let Some(item) = self.plan.items.get(position).cloned() else {
            self.ended = true;
            return;
        };
        let gain = item.gain.linear(&self.sound().levelling);
        match Deck::open(&item, seconds, self.rate, gain) {
            Some(deck) => {
                self.deck = Some(deck);
                self.mark(offset, item.id, seconds);
            }
            None => {
                self.ended = true;
                self.finished_sent = true;
                let _ = self.events.send(Event::Failed(item.id));
            }
        }
    }

    pub(super) fn mark(&self, offset: usize, id: String, seconds: f64) {
        let mut marks = lock(&self.shared.marks);
        marks.push(Mark {
            frame: self.scheduled + offset as i64,
            id,
            seconds,
        });
        if marks.len() > 8 {
            marks.remove(0);
        }
    }

    pub(super) fn check_boundary(&mut self) {
        let played = self.shared.played.load(Ordering::Relaxed) as i64;
        let id = {
            let marks = lock(&self.shared.marks);
            marks
                .iter()
                .rev()
                .find(|m| m.frame <= played)
                .map(|m| m.id.clone())
        };
        if let Some(id) = id {
            if self.notified.as_ref() != Some(&id) {
                self.notified = Some(id.clone());
                let _ = self.events.send(Event::Track(id));
            }
        }
    }

    pub(super) fn check_finished(&mut self) {
        if self.ended
            && !self.finished_sent
            && self.deck.is_none()
            && self.shared.played.load(Ordering::Relaxed) as i64 >= self.scheduled
        {
            self.finished_sent = true;
            let _ = self.events.send(Event::Finished);
        }
    }

    pub(super) fn apply_sound(&mut self) {
        let wanted = self.sound();
        if wanted == self.applied {
            return;
        }
        if wanted.effects != self.applied.effects {
            self.chain.configure(wanted.effects);
        }
        self.shared.dither.store(
            wanted.effects.dither && wanted.effects.active(),
            Ordering::Relaxed,
        );
        if wanted.curve != self.applied.curve
            || wanted.eq_on != self.applied.eq_on
            || wanted.headroom != self.applied.headroom
        {
            self.eq
                .configure(&wanted.curve, wanted.eq_on, wanted.headroom);
        }
        if wanted.master_db != self.applied.master_db {
            self.stage.configure(wanted.master_db);
        }
        if wanted.levelling != self.applied.levelling {
            for deck in [
                self.deck.as_mut(),
                self.outgoing.as_mut(),
                self.prepared.as_mut(),
            ]
            .into_iter()
            .flatten()
            {
                deck.gain = deck.item.gain.linear(&wanted.levelling);
            }
        }
        self.applied = wanted;
    }

    /// Opens the next song in time and starts the crossfade when this one is close enough to its end.
    pub(super) fn consider_fade(&mut self, done: usize) {
        if self.outgoing.is_some() || self.applied.crossfade <= 0.0 {
            return;
        }
        let Some(index) = self.plan.next(self.position) else {
            return;
        };
        if index == self.position {
            return;
        }
        let Some(item) = self.plan.items.get(index).cloned() else {
            return;
        };
        let Some(deck) = self.deck.as_ref() else {
            return;
        };
        let (left, total) = (deck.total - deck.read, deck.total);
        let rate = self.rate as f64;
        if self.prepared.is_none()
            && self.failed_next.as_ref() != Some(&item.id)
            && left <= ((self.applied.crossfade + 4.0) * rate) as i64
        {
            let gain = item.gain.linear(&self.applied.levelling);
            match Deck::open(&item, 0.0, self.rate, gain) {
                Some(next) => self.prepared = Some(next),
                None => self.failed_next = Some(item.id.clone()), // the ordinary change of song will report it
            }
        }
        let Some(next_total) = self
            .prepared
            .as_ref()
            .filter(|p| p.item.id == item.id)
            .map(|p| p.total)
        else {
            return;
        };
        // Never longer than half of either song.
        let fade = ((self.applied.crossfade * rate) as i64).min(total.min(next_total) / 2);
        if fade <= 0 || left > fade {
            return;
        }
        self.outgoing = self.deck.take();
        self.deck = self.prepared.take();
        self.position = index;
        self.fade_total = fade.min(left).max((rate / 5.0) as i64) as usize;
        self.fade_done = 0;
        self.mark(done, item.id, 0.0);
    }

    pub(super) fn finish_deck(&mut self, done: usize) {
        self.outgoing = None;
        self.fade_total = 0;
        self.deck = None;
        let Some(index) = self.plan.next(self.position) else {
            self.ended = true;
            return;
        };
        let id = self.plan.items.get(index).map(|i| i.id.clone());
        self.position = index;
        if self
            .prepared
            .as_ref()
            .map(|p| Some(p.item.id.clone()) == id)
            .unwrap_or(false)
        {
            self.deck = self.prepared.take();
            if let Some(id) = id {
                self.mark(done, id, 0.0);
            }
        } else {
            self.open(index, 0.0, done);
        }
    }

    pub(super) fn render_block(&mut self) {
        self.apply_sound();
        let limiting = self.applied.effects.active();
        let metering = self.shared.meter.load(Ordering::Relaxed);
        let original = self.applied.original;
        let mut done = 0;
        while done < BLOCK {
            if self.deck.is_none() {
                break;
            }
            self.consider_fade(done);
            let Some(current) = self.deck.as_mut() else {
                break;
            };
            if current.available() == 0 && !current.refill() {
                self.finish_deck(done);
                if self.ended {
                    break;
                }
                continue;
            }
            if let Some(fading) = self.outgoing.as_mut() {
                if fading.available() == 0 && !fading.refill() {
                    self.outgoing = None;
                }
            }
            let Some(current) = self.deck.as_mut() else {
                break;
            };
            let take = (BLOCK - done)
                .min(current.available())
                .min(self.outgoing.as_ref().map_or(usize::MAX, |d| d.available()));
            if take == 0 {
                continue;
            }
            let total = self.fade_total;
            for i in 0..take {
                let mut l = current.pending[current.at + 2 * i];
                let mut r = current.pending[current.at + 2 * i + 1];
                if current.gain != 1.0 {
                    l = (l * current.gain).clamp(-1.0, 1.0);
                    r = (r * current.gain).clamp(-1.0, 1.0);
                }
                if total > 0 {
                    let place = ((self.fade_done + i) as f64 / total as f64).min(1.0)
                        * std::f64::consts::FRAC_PI_2;
                    let rising = place.sin() as f32;
                    l *= rising;
                    r *= rising;
                    if let Some(fading) = self.outgoing.as_ref() {
                        let falling = place.cos() as f32;
                        let (mut old_l, mut old_r) = (
                            fading.pending[fading.at + 2 * i],
                            fading.pending[fading.at + 2 * i + 1],
                        );
                        if fading.gain != 1.0 {
                            old_l = (old_l * fading.gain).clamp(-1.0, 1.0);
                            old_r = (old_r * fading.gain).clamp(-1.0, 1.0);
                        }
                        l += old_l * falling;
                        r += old_r * falling;
                    }
                }
                self.dry_left[done + i] = l;
                self.dry_right[done + i] = r;
                self.chain.process(&mut l, &mut r, limiting);
                self.left[done + i] = l;
                self.right[done + i] = r;
                if metering {
                    let music = self.chain.music as f32;
                    if let Some((dry, wet)) = self
                        .meter
                        .push(music, if original { 0.0 } else { (l + r) / 2.0 - music })
                    {
                        let mut bars = lock(&self.shared.bars);
                        bars.push(Bars {
                            frame: self.scheduled + (done + i) as i64,
                            dry,
                            wet,
                        });
                        if bars.len() > 48 {
                            bars.remove(0);
                        }
                    }
                }
            }
            current.at += take * 2;
            current.read += take as i64;
            if let Some(fading) = self.outgoing.as_mut() {
                fading.at += take * 2;
                fading.read += take as i64;
            }
            if total > 0 {
                self.fade_done += take;
                if self.fade_done >= total {
                    self.outgoing = None;
                    self.fade_total = 0;
                }
            }
            done += take;
        }
        if done == 0 {
            return;
        }
        self.eq
            .process(&mut self.left[..done], &mut self.right[..done]);
        self.stage
            .process(&mut self.left[..done], &mut self.right[..done]);
        self.matcher.process(
            &self.dry_left[..done],
            &self.dry_right[..done],
            &mut self.left[..done],
            &mut self.right[..done],
            original,
        );
        for i in 0..done {
            let _ = self.producer.push(self.left[i]);
            let _ = self.producer.push(self.right[i]);
        }
        self.scheduled += done as i64;
    }
}

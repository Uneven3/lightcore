use bevy::prelude::*;

use crate::core::prelude::*;
use crate::gameplay::{
    CascadeDepth, ChainPop, PowerCombo, PowerConsumed, PowerCreated, ScoreDrained, SwapFailed,
};
use crate::state::GameState;

pub(crate) struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_sounds)
            .add_observer(on_chain_pop)
            .add_observer(on_power_consumed)
            .add_observer(on_power_combo)
            .add_observer(on_score_drained)
            .add_observer(on_swap_failed)
            .add_observer(on_power_created)
            .add_systems(OnEnter(GameState::LevelComplete), on_level_complete)
            .add_systems(OnEnter(GameState::GameOver), on_game_over);
    }
}

// ─── Sound handles ───────────────────────────────────────────────────────────

#[derive(Resource)]
pub(crate) struct SoundAssets {
    // Match pops
    pop_small: Handle<AudioSource>,    // 1-4 lights: crystalline pluck
    pop_big: Handle<AudioSource>,      // 5+ lights: richer pluck
    cascade_base: Handle<AudioSource>, // cascade chain (pitch scaled per wave)
    // Power upgrades / lifecycle
    power_created: Handle<AudioSource>,  // light upgraded to power
    score_drained: Handle<AudioSource>,  // Hollow consumed the score
    swap_failed: Handle<AudioSource>,    // swap reverted
    level_complete: Handle<AudioSource>, // level won
    game_over: Handle<AudioSource>,      // game lost
    // Individual power activation sounds
    ray_h: Handle<AudioSource>,     // horizontal row sweep
    ray_v: Handle<AudioSource>,     // vertical column sweep
    cross: Handle<AudioSource>,     // row+column together
    supernova: Handle<AudioSource>, // 3×3 ring burst
    starburst: Handle<AudioSource>, // color-targeting sparkle
    blackhole: Handle<AudioSource>, // board-consuming void
    // Individual combo sounds
    combo_double_line: Handle<AudioSource>,      // Ray × Ray
    combo_line_supernova: Handle<AudioSource>,   // Ray × Supernova
    combo_double_supernova: Handle<AudioSource>, // Supernova × Supernova
    combo_star_line: Handle<AudioSource>,        // Starburst × Ray
    combo_star_supernova: Handle<AudioSource>,   // Starburst × Supernova
    combo_star_color: Handle<AudioSource>,       // Starburst × Normal
    combo_star_star: Handle<AudioSource>,        // Starburst × Starburst
    combo_blackhole: Handle<AudioSource>,        // Blackhole × anything
    combo_super_combo: Handle<AudioSource>,      // 3+ powers simultaneously
}

fn setup_sounds(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    commands.insert_resource(SoundAssets {
        // Match pops
        pop_small: sources.add(make_chord(&[880.0, 1100.0], 0.10, 0.45)),
        pop_big: sources.add(make_chord(&[880.0, 1100.0, 1320.0], 0.14, 0.45)),
        cascade_base: sources.add(make_chord(&[1047.0, 1319.0], 0.14, 0.42)),
        // Power upgrades / lifecycle
        power_created: sources.add(make_arpeggio(
            &[(659.0, 0.04), (784.0, 0.04), (1047.0, 0.04), (1319.0, 0.06)],
            0.55,
        )),
        score_drained: sources.add(make_score_drain()),
        swap_failed: sources.add(make_sweep(320.0, 120.0, 0.10, 0.40)),
        level_complete: sources.add(make_arpeggio(
            &[
                (523.0, 0.06),
                (659.0, 0.06),
                (784.0, 0.06),
                (1047.0, 0.06),
                (1568.0, 0.10),
            ],
            0.55,
        )),
        game_over: sources.add(make_sweep(440.0, 60.0, 0.70, 0.55)),
        // Individual power activation sounds
        ray_h: sources.add(make_sweep(1600.0, 280.0, 0.12, 0.52)),
        ray_v: sources.add(make_sweep(1200.0, 220.0, 0.12, 0.52)),
        cross: sources.add(make_sweep(1900.0, 200.0, 0.20, 0.54)),
        supernova: sources.add(make_chord(&[180.0, 280.0, 420.0], 0.32, 0.56)),
        starburst: sources.add(make_chord(&[1047.0, 1319.0, 1760.0, 2200.0], 0.26, 0.48)),
        blackhole: sources.add(make_sweep(420.0, 30.0, 1.00, 0.56)),
        // Individual combo sounds
        combo_double_line: sources.add(make_sweep(2000.0, 220.0, 0.22, 0.54)),
        combo_line_supernova: sources.add(make_arpeggio(
            &[(1600.0, 0.10), (400.0, 0.05), (200.0, 0.24)],
            0.54,
        )),
        combo_double_supernova: sources.add(make_chord(&[140.0, 220.0, 340.0, 500.0], 0.44, 0.56)),
        combo_star_line: sources.add(make_arpeggio(
            &[(1760.0, 0.08), (1047.0, 0.06), (600.0, 0.18)],
            0.52,
        )),
        combo_star_supernova: sources.add(make_arpeggio(
            &[(1760.0, 0.08), (880.0, 0.06), (220.0, 0.26)],
            0.54,
        )),
        combo_star_color: sources.add(make_chord(&[1319.0, 1760.0, 2200.0], 0.22, 0.48)),
        combo_star_star: sources.add(make_chord(
            &[660.0, 880.0, 1047.0, 1320.0, 1760.0],
            0.50,
            0.56,
        )),
        combo_blackhole: sources.add(make_sweep(500.0, 25.0, 1.30, 0.58)),
        combo_super_combo: sources.add(make_arpeggio(
            &[
                (880.0, 0.05),
                (1047.0, 0.05),
                (1319.0, 0.05),
                (1568.0, 0.05),
                (2093.0, 0.12),
            ],
            0.60,
        )),
    });
}

// ─── Observers ───────────────────────────────────────────────────────────────

const CASCADE_PITCH_STEP: f32 = 0.08;
const CASCADE_PITCH_MAX: f32 = 2.0;

fn on_chain_pop(
    trigger: On<ChainPop>,
    cascade: Res<CascadeDepth>,
    sounds: Res<SoundAssets>,
    virtual_time: Res<Time<Virtual>>,
    mut commands: Commands,
) {
    if trigger.hollow {
        return;
    }
    let speed = virtual_time.relative_speed();
    if cascade.0 <= 1 {
        let handle = if trigger.removed >= 5 {
            sounds.pop_big.clone()
        } else {
            sounds.pop_small.clone()
        };
        play(&mut commands, handle, speed);
    } else {
        let pitch =
            (1.0 + cascade.0.saturating_sub(1) as f32 * CASCADE_PITCH_STEP).min(CASCADE_PITCH_MAX);
        play_pitched(&mut commands, sounds.cascade_base.clone(), pitch, speed);
    }
}

fn on_power_consumed(
    trigger: On<PowerConsumed>,
    sounds: Res<SoundAssets>,
    virtual_time: Res<Time<Virtual>>,
    mut commands: Commands,
) {
    use LightKind::*;
    let handle = match trigger.kind {
        RayH => sounds.ray_h.clone(),
        RayV => sounds.ray_v.clone(),
        Cross => sounds.cross.clone(),
        Supernova => sounds.supernova.clone(),
        Starburst => sounds.starburst.clone(),
        Blackhole => sounds.blackhole.clone(),
        Normal | Hollow => return,
    };
    play(&mut commands, handle, virtual_time.relative_speed());
}

fn on_power_combo(
    trigger: On<PowerCombo>,
    sounds: Res<SoundAssets>,
    virtual_time: Res<Time<Virtual>>,
    mut commands: Commands,
) {
    use ComboKind::*;
    let handle = match trigger.kind {
        DoubleLine => sounds.combo_double_line.clone(),
        LineSupernova => sounds.combo_line_supernova.clone(),
        DoubleSupernova => sounds.combo_double_supernova.clone(),
        StarLine => sounds.combo_star_line.clone(),
        StarSupernova => sounds.combo_star_supernova.clone(),
        StarColor => sounds.combo_star_color.clone(),
        StarStar => sounds.combo_star_star.clone(),
        Blackhole => sounds.combo_blackhole.clone(),
        SuperCombo => sounds.combo_super_combo.clone(),
    };
    play(&mut commands, handle, virtual_time.relative_speed());
}

fn on_score_drained(
    _: On<ScoreDrained>,
    sounds: Res<SoundAssets>,
    virtual_time: Res<Time<Virtual>>,
    mut commands: Commands,
) {
    play(
        &mut commands,
        sounds.score_drained.clone(),
        virtual_time.relative_speed(),
    );
}

fn on_swap_failed(
    _: On<SwapFailed>,
    sounds: Res<SoundAssets>,
    virtual_time: Res<Time<Virtual>>,
    mut commands: Commands,
) {
    play(
        &mut commands,
        sounds.swap_failed.clone(),
        virtual_time.relative_speed(),
    );
}

fn on_power_created(
    _: On<PowerCreated>,
    sounds: Res<SoundAssets>,
    virtual_time: Res<Time<Virtual>>,
    mut commands: Commands,
) {
    play(
        &mut commands,
        sounds.power_created.clone(),
        virtual_time.relative_speed(),
    );
}

fn on_level_complete(
    mut commands: Commands,
    sounds: Res<SoundAssets>,
    virtual_time: Res<Time<Virtual>>,
) {
    play(
        &mut commands,
        sounds.level_complete.clone(),
        virtual_time.relative_speed(),
    );
}

fn on_game_over(
    mut commands: Commands,
    sounds: Res<SoundAssets>,
    virtual_time: Res<Time<Virtual>>,
) {
    play(
        &mut commands,
        sounds.game_over.clone(),
        virtual_time.relative_speed(),
    );
}

// ─── Playback helpers ────────────────────────────────────────────────────────

pub(crate) fn play(commands: &mut Commands, handle: Handle<AudioSource>, speed: f32) {
    commands.spawn((
        AudioPlayer(handle),
        PlaybackSettings::DESPAWN.with_speed(speed),
    ));
}

pub(crate) fn play_pitched(
    commands: &mut Commands,
    handle: Handle<AudioSource>,
    pitch: f32,
    speed: f32,
) {
    commands.spawn((
        AudioPlayer(handle),
        PlaybackSettings::DESPAWN.with_speed(pitch * speed),
    ));
}

// ─── Synthesis ───────────────────────────────────────────────────────────────

const SR: u32 = 22050;

fn wav_header(data_size: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + data_size).to_le_bytes());
    h.extend_from_slice(b"WAVE");
    h.extend_from_slice(b"fmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes()); // PCM
    h.extend_from_slice(&1u16.to_le_bytes()); // mono
    h.extend_from_slice(&SR.to_le_bytes());
    h.extend_from_slice(&(SR * 2).to_le_bytes()); // byte rate
    h.extend_from_slice(&2u16.to_le_bytes()); // block align
    h.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data_size.to_le_bytes());
    h
}

/// Additive synthesis: sum of sine oscillators at each frequency, shared decay envelope.
fn make_chord(freqs: &[f32], duration_secs: f32, amplitude: f32) -> AudioSource {
    let n = (SR as f32 * duration_secs) as usize;
    let mut bytes = wav_header((n * 2) as u32);
    let norm = amplitude / freqs.len() as f32;
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let env = ((duration_secs - t) / duration_secs).max(0.0).sqrt();
        let sample: f32 = freqs
            .iter()
            .map(|&f| f32::sin(std::f32::consts::TAU * f * t))
            .sum::<f32>();
        let s = (sample * env * norm * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    AudioSource {
        bytes: bytes.into(),
    }
}

/// Phase-accurate linear frequency sweep from start_hz to end_hz.
fn make_sweep(start_hz: f32, end_hz: f32, duration_secs: f32, amplitude: f32) -> AudioSource {
    let n = (SR as f32 * duration_secs) as usize;
    let mut bytes = wav_header((n * 2) as u32);
    let mut phase: f32 = 0.0;
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let progress = t / duration_secs;
        let freq = start_hz + (end_hz - start_hz) * progress;
        let env = ((duration_secs - t) / duration_secs).max(0.0).sqrt();
        phase = (phase + freq / SR as f32).fract();
        let s =
            (f32::sin(std::f32::consts::TAU * phase) * env * amplitude * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    AudioSource {
        bytes: bytes.into(),
    }
}

/// Bad-event cue: a short downward void pull with a faint dissonant beating tone.
fn make_score_drain() -> AudioSource {
    let duration_secs = 0.46;
    let n = (SR as f32 * duration_secs) as usize;
    let mut bytes = wav_header((n * 2) as u32);
    let mut phase_low = 0.0f32;
    let mut phase_grit = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let progress = t / duration_secs;
        let env = ((duration_secs - t) / duration_secs).max(0.0).powf(0.35);
        let wobble = 0.55 + 0.45 * f32::sin(std::f32::consts::TAU * 18.0 * t);
        let low_freq = 170.0 + (42.0 - 170.0) * progress.powf(0.65);
        let grit_freq = 245.0 + (118.0 - 245.0) * progress;
        phase_low = (phase_low + low_freq / SR as f32).fract();
        phase_grit = (phase_grit + grit_freq / SR as f32).fract();
        let low = f32::sin(std::f32::consts::TAU * phase_low) * 0.72;
        let grit = f32::sin(std::f32::consts::TAU * phase_grit) * 0.28 * wobble;
        let s = ((low + grit) * env * 0.55 * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    AudioSource {
        bytes: bytes.into(),
    }
}

/// Sequence of (frequency_hz, duration_secs) pairs played back-to-back, each with its own envelope.
fn make_arpeggio(notes: &[(f32, f32)], amplitude: f32) -> AudioSource {
    let total: usize = notes.iter().map(|(_, d)| (SR as f32 * d) as usize).sum();
    let mut bytes = wav_header((total * 2) as u32);
    for &(freq, duration_secs) in notes {
        let n = (SR as f32 * duration_secs) as usize;
        for i in 0..n {
            let t = i as f32 / SR as f32;
            let env = ((duration_secs - t) / duration_secs).max(0.0).sqrt();
            let s = (f32::sin(std::f32::consts::TAU * freq * t) * env * amplitude * i16::MAX as f32)
                as i16;
            bytes.extend_from_slice(&s.to_le_bytes());
        }
    }
    AudioSource {
        bytes: bytes.into(),
    }
}

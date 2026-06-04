use crate::Plumbob;
use anyhow::Result;
use std::thread;
use std::time::Duration;

pub type Rgb = (u8, u8, u8);

pub struct Mood {
    pub name: &'static str,
    pub color: Rgb,
}

/// Sims 4 mood palette (used for the local CLI demo).
pub const SIMS4_MOODS: &[Mood] = &[
    Mood { name: "Happy",         color: (0x42, 0xd8, 0x54) },
    Mood { name: "Confident",     color: (0x00, 0xbf, 0xff) },
    Mood { name: "Energized",     color: (0xff, 0x9d, 0x00) },
    Mood { name: "Flirty",        color: (0xff, 0x69, 0xb4) },
    Mood { name: "Focused",       color: (0x1e, 0x90, 0xff) },
    Mood { name: "Playful",       color: (0xff, 0x00, 0xff) },
    Mood { name: "Inspired",      color: (0x00, 0xff, 0xc8) },
    Mood { name: "Sad",           color: (0x20, 0x40, 0xff) },
    Mood { name: "Embarrassed",   color: (0xff, 0x55, 0x55) },
    Mood { name: "Angry",         color: (0xff, 0x00, 0x00) },
    Mood { name: "Tense",         color: (0xff, 0x40, 0x10) },
    Mood { name: "Uncomfortable", color: (0xff, 0xff, 0x00) },
    Mood { name: "Dazed",         color: (0x94, 0x00, 0xd3) },
    Mood { name: "Asleep",        color: (0x00, 0x00, 0x00) },
];

/// Map a Paralives emotion DisplayName to an RGB color. Set to the 11 emotions
/// present in Paralives Early Access (2026-06). Names match the in-game DisplayName
/// (case-insensitive). Flirty/Uncomfortable get the BackgroundColor from the game's
/// own Setting.Emotion; the rest inherit positive/negative defaults so we pick a
/// color that fits each emotion thematically.
pub fn paralives_emotion(name: &str) -> Option<Rgb> {
    Some(match name.to_ascii_lowercase().as_str() {
        "happy"        => (0x42, 0xd8, 0x54),  // green
        "amused"       => (0xff, 0xc0, 0x40),  // amber
        "inspired"     => (0x00, 0xff, 0xc8),  // mint
        "flirty"       => (0xe1, 0x59, 0x7f),  // pink (from in-game BackgroundColor)
        "sad"          => (0x20, 0x40, 0xff),  // blue
        "angry"        => (0xff, 0x00, 0x00),  // red
        "stressed"     => (0xff, 0x40, 0x10),  // red-orange
        "bored"        => (0x80, 0x80, 0x80),  // grey
        "disgust" | "disgusted" => (0x80, 0xa0, 0x20),  // sickly green
        "embarrassed"  => (0xff, 0x55, 0x55),  // light red
        "uncomfortable"=> (0xde, 0x3e, 0x3b),  // red (from in-game BackgroundColor)
        _ => return None,
    })
}

pub fn fade(bob: &mut Plumbob, from: Rgb, to: Rgb, duration: Duration) -> Result<()> {
    const FPS: u64 = 60;
    let steps = (duration.as_millis() as u64 * FPS / 1000).max(1);
    let frame = Duration::from_millis(1000 / FPS);

    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        bob.set_rgb(lerp(from.0, to.0), lerp(from.1, to.1), lerp(from.2, to.2))?;
        thread::sleep(frame);
    }
    Ok(())
}

pub fn mood_cycle(bob: &mut Plumbob, hold: Duration, fade_dur: Duration) -> Result<()> {
    let mut current: Rgb = (0, 0, 0);
    loop {
        for mood in SIMS4_MOODS {
            println!("→ {}", mood.name);
            fade(bob, current, mood.color, fade_dur)?;
            current = mood.color;
            thread::sleep(hold);
        }
    }
}

//! The voices we ship, and the pace each one has to be slowed to.
//!
//! Voices read at different native speeds — 176 to 201 words per minute across ours — so a single
//! stretch factor would make one voice brisk and another sluggish. Each voice therefore carries its
//! own measured rate, and the tempo is solved from the pace the reader approved.

/// A shipped voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    pub name: &'static str,
    pub accent: Accent,
    pub female: bool,
    /// Words per minute this voice reads at untouched. Measured on Psalm 23:1–3, not guessed.
    pub native_wpm: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accent {
    UnitedStates,
    UnitedKingdom,
}

/// The pace approved by ear: Edwin reading at 0.80, which is 190.6 × 0.80 = 152.5 wpm of speech.
pub const SPEECH_WPM: f32 = 152.5;

pub const VOICES: [Voice; 4] = [
    Voice { name: "Russell", accent: Accent::UnitedStates, female: false, native_wpm: 201 },
    Voice { name: "Spencer", accent: Accent::UnitedKingdom, female: false, native_wpm: 177 },
    Voice { name: "Harper", accent: Accent::UnitedStates, female: true, native_wpm: 189 },
    Voice { name: "Freya", accent: Accent::UnitedKingdom, female: true, native_wpm: 178 },
];

impl Voice {
    /// The stretch factor that lands this voice on the approved pace.
    pub fn tempo(self) -> f32 {
        SPEECH_WPM / self.native_wpm as f32
    }

    /// The voice for a reader whose device is set to this language tag, or the UK man by default.
    pub fn for_locale(locale: &str) -> Voice {
        let tag = locale.to_ascii_lowercase().replace('-', "_");
        match tag.as_str() {
            "en_us" => VOICES[0],
            _ => VOICES[1],
        }
    }

    pub fn by_name(name: &str) -> Option<Voice> {
        VOICES.iter().copied().find(|v| v.name.eq_ignore_ascii_case(name))
    }

    /// "English (US) · male", the second line of a settings row.
    pub fn describe(self) -> String {
        let place = match self.accent {
            Accent::UnitedStates => "US",
            Accent::UnitedKingdom => "UK",
        };
        let sex = if self.female { "female" } else { "male" };
        format!("English ({place}) · {sex}")
    }
}

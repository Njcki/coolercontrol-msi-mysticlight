// SPDX-License-Identifier: GPL-3.0-or-later
//! MSI Mystic Light 185-byte feature report layout and the zone-based (non direct) write logic.
//!
//! Adapted from OpenRGB (GPL-2.0-or-later, https://gitlab.com/CalcProgrammer1/OpenRGB):
//! Controllers/MSIMotherboardController/MSIMotherboardCommon.h and
//! MSIMotherboard185Controller/MSIMotherboard185Controller.cpp
//! (T-bond, Adam Honse and the OpenRGB contributors). Only the zone-based path used by OpenRGB
//! for effect modes is reproduced here; the per-LED "direct" mode (report 0x53) is not used.
//!
//! Board config used by OpenRGB for PID 0x0076 (common PID, virtual PID 0x7E16 taken from the
//! serial number = X670E GAMING PLUS WIFI): 0 onboard LEDs, 2 JRGB, zones JRGB1, JRGB2,
//! JRAINBOW1, JRAINBOW2, DIRECT_MODE_PER_LED, mixed-mode support (PID >= 0x7D03).

#![allow(dead_code)] // full layout kept for reference

pub const REPORT_ID: u8 = 0x52;
pub const PACKET_LEN: usize = 185;

// byte offsets inside the 185 byte packet (byte 0 = report id)
pub const OFF_J_RGB_1: usize = 1;
pub const OFF_J_PIPE_1: usize = 11;
pub const OFF_J_PIPE_2: usize = 21;
pub const OFF_J_RAINBOW_1: usize = 31; // RainbowZoneData: 10 bytes ZoneData + cycle_or_led_num
pub const OFF_J_RAINBOW_2: usize = 42;
pub const OFF_J_CORSAIR: usize = 53;
pub const OFF_ON_BOARD_LED: usize = 74;
pub const OFF_ON_BOARD_LED_6: usize = 134; // JRGB2 on mixed-mode boards
pub const OFF_J_RGB_2: usize = 174;
pub const OFF_SAVE: usize = 184;

// ZoneData field offsets
const Z_EFFECT: usize = 0;
const Z_COLOR: usize = 1;
const Z_SPEED_BRIGHT: usize = 4;
const Z_COLOR2: usize = 5;
const Z_COLOR_FLAGS: usize = 8;
const Z_PADDING: usize = 9;
const Z_CYCLE_OR_LED_NUM: usize = 10;

pub const MSI_DIRECT_MODE: u8 = 0x25;
pub const FLAG_SYNC_SETTING_JRGB: u8 = 1 << 7; // in speedAndBrightnessFlags
pub const COLOR_FLAG_SYNC_JPIPE1: u8 = 1 << 4;
pub const COLOR_FLAG_SYNC_JPIPE2: u8 = 1 << 5;
pub const COLOR_FLAG_USE_CUSTOM_COLOR: u8 = 1 << 7;
pub const BRIGHTNESS_SHIFT: u8 = 2;
pub const BRIGHTNESS_MASK: u8 = 0x1F << BRIGHTNESS_SHIFT;
pub const SPEED_MASK: u8 = 0x03;
pub const BRIGHTNESS_100: u8 = 10;
pub const MODE_LIGHTNING: u8 = 5;
pub const MODE_STATIC: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    JRainbow1,
    JRainbow2,
}

impl Zone {
    pub fn offset(self) -> usize {
        match self {
            Zone::JRainbow1 => OFF_J_RAINBOW_1,
            Zone::JRainbow2 => OFF_J_RAINBOW_2,
        }
    }
    pub fn len(self) -> usize {
        11
    }
    pub fn name(self) -> &'static str {
        match self {
            Zone::JRainbow1 => "JRAINBOW1",
            Zone::JRainbow2 => "JRAINBOW2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colors {
    /// mode needs exactly one color
    One,
    /// one color, or none = the board's random/rainbow colors
    OneOrRandom,
    /// only the board's random/rainbow colors
    Random,
    /// static black
    Off,
}

pub struct ModeDef {
    pub name: &'static str,
    pub label: &'static str,
    pub value: u8,
    pub colors: Colors,
    pub speed: bool,
}

/// The modes OpenRGB enables for the 185-byte controller (RGBController_MSIMotherboard185::SetupModes),
/// plus "off" implemented as static black (OpenRGB keeps MSI_MODE_DISABLE commented out).
pub const MODES: &[ModeDef] = &[
    ModeDef { name: "fixed", label: "Fixed", value: 1, colors: Colors::One, speed: false },
    ModeDef { name: "off", label: "Off", value: 1, colors: Colors::Off, speed: false },
    ModeDef { name: "breathing", label: "Breathing", value: 2, colors: Colors::One, speed: true },
    ModeDef { name: "flashing", label: "Flashing", value: 3, colors: Colors::OneOrRandom, speed: true },
    ModeDef { name: "double_flashing", label: "Double Flashing", value: 4, colors: Colors::OneOrRandom, speed: true },
    ModeDef { name: "lightning", label: "Lightning", value: 5, colors: Colors::One, speed: true },
    ModeDef { name: "meteor", label: "Meteor", value: 7, colors: Colors::OneOrRandom, speed: true },
    ModeDef { name: "color_ring", label: "Color Ring (rainbow)", value: 15, colors: Colors::Random, speed: true },
    ModeDef { name: "planetary", label: "Planetary", value: 16, colors: Colors::Random, speed: true },
    ModeDef { name: "double_meteor", label: "Double Meteor", value: 17, colors: Colors::Random, speed: true },
    ModeDef { name: "energy", label: "Energy", value: 18, colors: Colors::Random, speed: true },
    ModeDef { name: "blink", label: "Blink", value: 19, colors: Colors::OneOrRandom, speed: true },
    ModeDef { name: "clock", label: "Clock", value: 20, colors: Colors::Random, speed: true },
    ModeDef { name: "color_pulse", label: "Color Pulse", value: 21, colors: Colors::OneOrRandom, speed: true },
    ModeDef { name: "color_shift", label: "Color Shift (color cycle)", value: 22, colors: Colors::Random, speed: true },
    ModeDef { name: "color_wave", label: "Color Wave", value: 23, colors: Colors::OneOrRandom, speed: true },
    ModeDef { name: "marquee", label: "Marquee", value: 24, colors: Colors::One, speed: true },
    ModeDef { name: "rainbow_wave", label: "Rainbow Wave", value: 26, colors: Colors::Random, speed: true },
    ModeDef { name: "visor", label: "Visor", value: 27, colors: Colors::OneOrRandom, speed: true },
    ModeDef { name: "rainbow_flashing", label: "Rainbow Flashing", value: 29, colors: Colors::Random, speed: true },
    ModeDef { name: "color_ring_double_flashing", label: "Color Ring Double Flashing", value: 35, colors: Colors::Random, speed: true },
    ModeDef { name: "stack", label: "Stack", value: 36, colors: Colors::OneOrRandom, speed: true },
    ModeDef { name: "fire", label: "Fire", value: 38, colors: Colors::Random, speed: true },
];

pub const SPEEDS: &[&str] = &["slow", "medium", "fast"];

pub fn find_mode(name: &str) -> Option<&'static ModeDef> {
    MODES.iter().find(|m| m.name == name)
}

pub fn mode_name(value: u8) -> &'static str {
    match value {
        0 => "disable",
        MSI_DIRECT_MODE => "direct (per-LED)",
        v => MODES.iter().filter(|m| m.colors != Colors::Off).find(|m| m.value == v).map(|m| m.name).unwrap_or("other"),
    }
}

/// Parsed request for one zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneSetting {
    pub mode: u8,
    pub speed: u8,      // 0 low, 1 medium, 2 high
    pub random: bool,   // board colors instead of the custom color
    pub color: [u8; 3],
}

impl ZoneSetting {
    pub fn from_request(mode: &str, speed: Option<&str>, colors: &[[u8; 3]]) -> Result<Self, String> {
        let def = find_mode(mode).ok_or_else(|| format!("unknown lighting mode '{mode}'"))?;
        // OpenRGB: modes with speed default to MEDIUM, modes without speed use 0
        let speed = if def.speed {
            match speed.map(|s| s.to_ascii_lowercase()) {
                None => 1,
                Some(s) => SPEEDS.iter().position(|x| *x == s).ok_or_else(|| format!("unknown speed '{s}'"))? as u8,
            }
        } else {
            0
        };
        let first = colors.first().copied();
        let (random, color) = match def.colors {
            Colors::Off => (false, [0, 0, 0]),
            Colors::Random => (true, [0, 0, 0]),
            Colors::One => (false, first.ok_or("this mode needs one color")?),
            Colors::OneOrRandom => match first {
                Some(c) => (false, c),
                None => (true, [0, 0, 0]),
            },
        };
        Ok(Self { mode: def.value, speed, random, color })
    }
}

pub fn validate(packet: &[u8]) -> Result<(), String> {
    if packet.len() != PACKET_LEN {
        return Err(format!("expected {PACKET_LEN} byte report, got {}", packet.len()));
    }
    if packet[0] != REPORT_ID {
        return Err(format!("unexpected report id 0x{:02x}", packet[0]));
    }
    Ok(())
}

/// Applies a zone setting to a packet the same way OpenRGB's MSIMotherboard185Controller::SetMode +
/// SetZoneColor do for a JRAINBOW zone on a board without onboard LEDs (no_onboards = true):
/// - zone effect, speed/brightness (brightness 100%), custom-color flag, padding 0
/// - for modes above LIGHTNING the onboard zone syncs JRGB/JPIPE1/JPIPE2, otherwise those sync bits are cleared
/// - color and color2 = requested color
/// cycle_or_led_num is left untouched (keeps the value configured on the board).
pub fn apply_zone(packet: &mut [u8], zone: Zone, s: &ZoneSetting) {
    let z = zone.offset();
    packet[z + Z_EFFECT] = s.mode;
    packet[z + Z_SPEED_BRIGHT] =
        ((BRIGHTNESS_100 << BRIGHTNESS_SHIFT) & BRIGHTNESS_MASK) | (s.speed & SPEED_MASK);
    if s.random {
        packet[z + Z_COLOR_FLAGS] &= !COLOR_FLAG_USE_CUSTOM_COLOR;
    } else {
        packet[z + Z_COLOR_FLAGS] |= COLOR_FLAG_USE_CUSTOM_COLOR;
    }
    packet[z + Z_PADDING] = 0;

    let ob = OFF_ON_BOARD_LED;
    if s.mode > MODE_LIGHTNING {
        packet[ob + Z_SPEED_BRIGHT] |= FLAG_SYNC_SETTING_JRGB;
        packet[ob + Z_COLOR_FLAGS] |= COLOR_FLAG_SYNC_JPIPE1 | COLOR_FLAG_SYNC_JPIPE2;
    } else {
        packet[ob + Z_SPEED_BRIGHT] &= !FLAG_SYNC_SETTING_JRGB;
        packet[ob + Z_COLOR_FLAGS] &= !(COLOR_FLAG_SYNC_JPIPE1 | COLOR_FLAG_SYNC_JPIPE2);
    }

    packet[z + Z_COLOR..z + Z_COLOR + 3].copy_from_slice(&s.color);
    packet[z + Z_COLOR2..z + Z_COLOR2 + 3].copy_from_slice(&s.color);
    packet[OFF_SAVE] = 0;
}

/// Copies one zone (and the onboard sync bits) back from a reference packet.
pub fn restore_zone(packet: &mut [u8], reference: &[u8], zone: Zone) {
    let z = zone.offset();
    packet[z..z + zone.len()].copy_from_slice(&reference[z..z + zone.len()]);
    let ob = OFF_ON_BOARD_LED;
    packet[ob + Z_SPEED_BRIGHT] = (packet[ob + Z_SPEED_BRIGHT] & !FLAG_SYNC_SETTING_JRGB)
        | (reference[ob + Z_SPEED_BRIGHT] & FLAG_SYNC_SETTING_JRGB);
    let m = COLOR_FLAG_SYNC_JPIPE1 | COLOR_FLAG_SYNC_JPIPE2;
    packet[ob + Z_COLOR_FLAGS] = (packet[ob + Z_COLOR_FLAGS] & !m) | (reference[ob + Z_COLOR_FLAGS] & m);
    packet[OFF_SAVE] = 0;
}

pub fn describe_zone(packet: &[u8], name: &str, off: usize, rainbow: bool) -> String {
    let z = &packet[off..];
    let sb = z[Z_SPEED_BRIGHT];
    let mut s = format!(
        "{name:<10} effect={:>2} ({:<16}) color=#{:02x}{:02x}{:02x} color2=#{:02x}{:02x}{:02x} speed={} brightness={} custom_color={} flags=0x{:02x} pad=0x{:02x}",
        z[Z_EFFECT],
        mode_name(z[Z_EFFECT]),
        z[1], z[2], z[3], z[5], z[6], z[7],
        sb & SPEED_MASK,
        (sb & BRIGHTNESS_MASK) >> BRIGHTNESS_SHIFT,
        z[Z_COLOR_FLAGS] & COLOR_FLAG_USE_CUSTOM_COLOR != 0,
        z[Z_COLOR_FLAGS],
        z[Z_PADDING],
    );
    if rainbow {
        s.push_str(&format!(" leds={}", z[Z_CYCLE_OR_LED_NUM]));
    }
    s
}

pub fn describe(packet: &[u8]) -> String {
    [
        describe_zone(packet, "JRGB1", OFF_J_RGB_1, false),
        describe_zone(packet, "JRGB2", OFF_ON_BOARD_LED_6, false),
        describe_zone(packet, "JRAINBOW1", OFF_J_RAINBOW_1, true),
        describe_zone(packet, "JRAINBOW2", OFF_J_RAINBOW_2, true),
        describe_zone(packet, "ONBOARD", OFF_ON_BOARD_LED, false),
        format!("save byte = {}", packet[OFF_SAVE]),
    ]
    .join("\n")
}

pub fn hex(packet: &[u8]) -> String {
    packet.chunks(16).map(|c| c.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut p = vec![0u8; PACKET_LEN];
        p[0] = REPORT_ID;
        for (i, b) in p.iter_mut().enumerate().skip(1) {
            *b = (i as u8).wrapping_mul(7);
        }
        p[OFF_SAVE] = 0;
        p
    }

    #[test]
    fn layout_matches_openrgb_struct() {
        // FeaturePacket_185 comments: j_rainbow_1 @31, j_rainbow_2 @42, j_corsair @53, on_board_led @74, j_rgb_2 @174, save @184
        assert_eq!(OFF_J_RAINBOW_1 + 11, OFF_J_RAINBOW_2);
        assert_eq!(OFF_J_RAINBOW_2 + 11, OFF_J_CORSAIR);
        assert_eq!(OFF_ON_BOARD_LED + 6 * 10, OFF_ON_BOARD_LED_6);
        assert_eq!(OFF_J_RGB_2 + 10, OFF_SAVE);
        assert_eq!(OFF_SAVE + 1, PACKET_LEN);
    }

    #[test]
    fn fixed_color_only_touches_zone_and_onboard_sync() {
        let orig = sample();
        let mut p = orig.clone();
        let s = ZoneSetting::from_request("fixed", None, &[[255, 0, 16]]).unwrap();
        apply_zone(&mut p, Zone::JRainbow2, &s);
        assert_eq!(&p[42..52], &[1, 255, 0, 16, 40, 255, 0, 16, orig[50] | 0x80, 0]);
        assert_eq!(p[52], orig[52], "led count kept");
        for i in 0..PACKET_LEN {
            if !(42..52).contains(&i) && i != 78 && i != 82 {
                assert_eq!(p[i], orig[i], "byte {i} changed");
            }
        }
        assert_eq!(p[78], orig[78] & !0x80);
        assert_eq!(p[82], orig[82] & !0x30);
    }

    #[test]
    fn rainbow_mode_uses_random_colors_and_sync() {
        let mut p = sample();
        let s = ZoneSetting::from_request("rainbow_wave", Some("fast"), &[]).unwrap();
        apply_zone(&mut p, Zone::JRainbow1, &s);
        assert_eq!(p[31], 26);
        assert_eq!(p[35], 40 | 2);
        assert_eq!(p[39] & 0x80, 0);
        assert_eq!(p[78] & 0x80, 0x80);
        assert_eq!(p[82] & 0x30, 0x30);
    }

    #[test]
    fn restore_zone_roundtrip() {
        let orig = sample();
        let mut p = orig.clone();
        apply_zone(&mut p, Zone::JRainbow1, &ZoneSetting::from_request("color_shift", None, &[]).unwrap());
        restore_zone(&mut p, &orig, Zone::JRainbow1);
        assert_eq!(p, orig);
    }

    #[test]
    fn request_parsing() {
        assert!(ZoneSetting::from_request("fixed", None, &[]).is_err());
        assert!(ZoneSetting::from_request("nope", None, &[]).is_err());
        let off = ZoneSetting::from_request("off", None, &[[9, 9, 9]]).unwrap();
        assert_eq!(off, ZoneSetting { mode: 1, speed: 0, random: false, color: [0, 0, 0] });
        let f = ZoneSetting::from_request("flashing", None, &[]).unwrap();
        assert!(f.random);
        assert_eq!(f.speed, 1);
    }
}

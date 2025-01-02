use midi::*;

pub const PAD_RELEASED_BRIGHTNESS: f32 = 0.015;

#[allow(dead_code)]
pub enum PressureShape {
    Linear,
    Exponential(f32),
    Constant(f32),
}

pub const PAD_NOTE_MAP: [U7; 16] = [12, 13, 14, 15, 8, 9, 10, 11, 4, 5, 6, 7, 0, 1, 2, 3];

pub fn usage(prog_name: &String) {
    println!("usage: {} <hidraw device>", prog_name);
}

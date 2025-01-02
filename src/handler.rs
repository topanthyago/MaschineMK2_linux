use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime};
use alsa_seq::*;

extern crate nix;

// extern crate hsl;
// use handler::HSL;

use nix::poll::*;
use hsl::HSL;
use midi::*;
use nix::poll::PollFd;
use base::{Maschine, MaschineButton, MaschineHandler};
use utils::{PressureShape, PAD_NOTE_MAP, PAD_RELEASED_BRIGHTNESS};


use std::os::unix::io::AsRawFd;

use tinyosc::{self as osc, osc_args};



use std::net::{Ipv4Addr, SocketAddrV4};

use crate::osc::{btn_to_osc_button_map, osc_button_to_btn_map};


pub struct MHandler<'a> {
    pub color: HSL,
    pub seq_handle: &'a SequencerHandle,
    pub seq_port: &'a SequencerPort<'a>,
    pub pressure_shape: PressureShape,
    pub send_aftertouch: bool,
    pub osc_socket: &'a UdpSocket,
    pub osc_outgoing_addr: SocketAddr,
}

impl<'a> MHandler<'a> {
    pub fn new(seq_handle: &'a SequencerHandle, seq_port: &'a SequencerPort<'a>, osc_socket: &'a UdpSocket) -> Self {
        MHandler {
            color: HSL { h: 0.0, s: 1.0, l: 0.3 },
            seq_handle,
            seq_port,
            pressure_shape: PressureShape::Exponential(0.4),
            send_aftertouch: false,
            osc_socket,
            osc_outgoing_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 42435)),
        }
    }

    pub fn pad_color(&self) -> u32 {
        let (r, g, b) = self.color.to_rgb();

        ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    }

    pub fn pressure_to_vel(&self, pressure: f32) -> U7 {
        (match self.pressure_shape {
            PressureShape::Linear => pressure,
            PressureShape::Exponential(power) => pressure.powf(power),
            PressureShape::Constant(c_pressure) => c_pressure,
        } * 127.0) as U7
    }

    #[allow(dead_code)]
    pub fn update_pad_colors(&self, maschine: &mut dyn Maschine) {
        for i in 0..16 {
            let brightness = match maschine.get_pad_pressure(i).unwrap() {
                b if b == 0.0 => PAD_RELEASED_BRIGHTNESS,
                pressure @ _ => pressure.sqrt(),
            };

            maschine.set_pad_light(i, self.pad_color(), brightness);
        }
    }



    pub fn recv_osc_msg(&self, maschine: &mut dyn Maschine) {
        let mut buf = [0u8; 128];

        let nbytes = match self.osc_socket.recv_from(&mut buf) {
            Ok((nbytes, _)) => nbytes,
            Err(e) => {
                println!(" :: error in recv_from(): {}", e);
                return;
            }
        };

        let msg = match osc::Message::deserialize(&buf[..nbytes]) {
            Ok(msg) => msg,
            Err(_) => {
                println!(" :: couldn't decode OSC message :c");
                return;
            }
        };

        self.handle_osc_messge(maschine, &msg);
    }

    pub  fn handle_osc_messge(&self, maschine: &mut dyn Maschine, msg: &osc::Message) {
        if msg.path.starts_with("/maschine/button") {
            let btn = match osc_button_to_btn_map(&msg.path[17..]) {
                Some(btn) => btn,
                None => return,
            };

            match msg.arguments.len() {
                1 => maschine.set_button_light(
                    btn,
                    0xFFFFFF,
                    match msg.arguments[0] {
                        osc::Argument::i(val) => val as f32,
                        osc::Argument::f(val) => val,
                        _ => return,
                    },
                ),

                2 => {
                    if let (&osc::Argument::i(color), &osc::Argument::f(brightness)) =
                        (&msg.arguments[0], &msg.arguments[1])
                    {
                        maschine.set_button_light(btn, (color as u32) & 0xFFFFFF, brightness);
                    }
                }

                _ => return,
            };
        } else if msg.path.starts_with("/maschine/pad") {
            match msg.arguments.len() {
                3 => {
                    if let (
                        &osc::Argument::i(pad),
                        &osc::Argument::i(color),
                        &osc::Argument::f(brightness),
                    ) = (&msg.arguments[0], &msg.arguments[1], &msg.arguments[2])
                    {
                        maschine.set_pad_light(
                            pad as usize,
                            (color as u32) & 0xFFFFFF,
                            brightness as f32,
                        );
                    }
                }

                _ => return,
            }
        } else if msg.path.starts_with("/maschine/midi_note_base") {
            match msg.arguments.len() {
                1 => {
                    if let osc::Argument::i(base) = msg.arguments[0] {
                        maschine.set_midi_note_base(base as u8);
                    }
                }
                _ => return,
            }
        }
    }

    pub fn send_osc_msg(&self, path: &str, arguments: Vec<osc::Argument>) {
        let msg = osc::Message {
            path: path,
            arguments: arguments,
        };

        match self
            .osc_socket
            .send_to(&*msg.serialize().unwrap(), &self.osc_outgoing_addr)
        {
            Ok(_) => {}
            Err(e) => println!(" :: error in send_to: {}", e),
        }
    }

    pub fn send_osc_button_msg(
        &mut self,
        maschine: &mut dyn Maschine,
        btn: MaschineButton,
        status: usize,
        is_down: bool,
    ) {
        let button = btn_to_osc_button_map(btn);
        let controlbase = 15;
        let modpress = maschine.get_mod();
        if button.contains("shift") {
            if status > 0 {
                maschine.set_mod(1);
            } else {
                maschine.set_mod(0);
            }
        }
        if button.contains("C") {
            let idx = 1;
            if button == "C8" {
                maschine.set_roller_state(status, idx);
                //println!("3={}", status);
            };
            if button == "C7" {
                maschine.set_roller_state(status, idx);
                //println!("2={}", status);
            };
        };
        if button.contains("E") {
            let idx = 2;
            if button == "E8" {
                maschine.set_roller_state(status, idx);
                //println!("3={}", status);
            };
        };
        if button.contains("G") {
            let idx = 3;
            if button == "G8" {
                maschine.set_roller_state(status, idx);
                //println!("3={}", status);
            };
        };
        if button.contains("I") {
            let idx = 4;
            if button == "I8" {
                maschine.set_roller_state(status, idx);
                //println!("3={}", status);
            };
        };
        if button.contains("K") {
            let idx = 5;
            if button == "K8" {
                maschine.set_roller_state(status, idx);
                //println!("3={}", status);
            };
        };
        if button.contains("M") {
            let idx = 6;
            if button == "M8" {
                maschine.set_roller_state(status, idx);
                //println!("3={}", status);
            };
        };
        if button.contains("O") {
            let idx = 7;
            if button == "O8" {
                maschine.set_roller_state(status, idx);
                //println!("3={}", status);
            };
        };
        if button.contains("A8") {
            let msg = Message::RPN7(Ch1, controlbase, status as u8 * 8);
            self.seq_port.send_message(&msg).unwrap();
            self.seq_handle.drain_output();
        }

        if is_down == true && status <= 250 {
            match button {
                "play" => {
                    if status > 0 && maschine.get_padmode() != 2 {
                        let msg = Message::RPN7(Ch1, 1, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    } else if maschine.get_padmode() == 2 {
                        maschine.set_playing(1);
                        println!("playing notes");
                    };
                }

                "stop" => {
                    if status > 0 && maschine.get_padmode() != 2 {
                        let msg = Message::RPN7(Ch1, 2, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    } else {
                        maschine.set_playing(0);
                        println!("stop");
                        //let msg2 = Message::AllNotesOff(Ch2);
                        //self.seq_port.send_message(&msg2).unwrap();
                        //self.seq_handle.drain_output();
                    }
                }
                "rec" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 3, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "grid" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 4, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "step_left" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 5, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "step_right" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 6, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "restart" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 7, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "browse" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 8, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "sampling" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 9, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "note_repeat" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 10, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "control" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 11, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "nav" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 12, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "nav_left" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 13, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "nav_right" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 14, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "main" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 24, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "scene" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 25, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "pattern" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 26, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "pad_mode" => {
                    if modpress == 1 {
                        maschine.set_padmode(1);
                    } else {
                        if status > 0 {
                            let msg = Message::RPN7(Ch1, 27, status as u8);
                            self.seq_port.send_message(&msg).unwrap();
                            self.seq_handle.drain_output();
                        }
                    }
                }
                "view" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 28, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "duplicate" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 29, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "select" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 30, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "solo" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 31, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "step" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 32, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "mute" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 33, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "navigate" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 34, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "tempo" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 35, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "enter" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 36, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "auto" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 37, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "all" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 38, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "f1" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 39, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "f2" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 40, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "f3" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 41, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "f4" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 42, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "f5" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 43, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "f6" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 44, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "f7" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 45, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "f8" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 46, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "page_right" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 47, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }
                "page_left" => {
                    if status > 0 {
                        let msg = Message::RPN7(Ch1, 48, status as u8);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    }
                }

                "B6" => {
                    let idx = 1;
                    let state = maschine.get_roller_state(idx);
                    let status = status / 4 + state * 64;
                    if modpress != 1 {
                        let msg = Message::RPN14(Ch1, controlbase + 1, status as u16 / 2);
                        self.seq_port.send_message(&msg).unwrap();
                        self.seq_handle.drain_output();
                    } else {
                        maschine.set_seq_speed(status);
                    }
                }
                "D6" => {
                    let idx = 2;
                    let state = maschine.get_roller_state(idx);
                    let status = status / 4 + state * 64;
                    let msg = Message::RPN14(Ch1, controlbase + 2, status as u16 / 2);
                    self.seq_port.send_message(&msg).unwrap();
                    self.seq_handle.drain_output();
                }
                "FF6" => {
                    let idx = 3;
                    let state = maschine.get_roller_state(idx);
                    let status = status / 4 + state * 64;
                    let msg = Message::RPN14(Ch1, controlbase + 3, status as u16 / 2);
                    self.seq_port.send_message(&msg).unwrap();
                    self.seq_handle.drain_output();
                }
                "H6" => {
                    let idx = 4;
                    let state = maschine.get_roller_state(idx);
                    let status = status / 4 + state * 64;
                    let msg = Message::RPN14(Ch1, controlbase + 4, status as u16 / 2);
                    self.seq_port.send_message(&msg).unwrap();
                    self.seq_handle.drain_output();
                }
                "J6" => {
                    let idx = 5;
                    let state = maschine.get_roller_state(idx);
                    let status = status / 4 + state * 64;
                    let msg = Message::RPN14(Ch1, controlbase + 5, status as u16 / 2);
                    self.seq_port.send_message(&msg).unwrap();
                    self.seq_handle.drain_output();
                }
                "L6" => {
                    let idx = 6;
                    let state = maschine.get_roller_state(idx);
                    let status = status / 4 + state * 64;
                    let msg = Message::RPN14(Ch1, controlbase + 6, status as u16 / 2);
                    self.seq_port.send_message(&msg).unwrap();
                    self.seq_handle.drain_output();
                }
                "N6" => {
                    let idx = 7;
                    let state = maschine.get_roller_state(idx);
                    let status = status / 4 + state * 64;
                    let msg = Message::RPN14(Ch1, controlbase + 7, status as u16 / 2);
                    self.seq_port.send_message(&msg).unwrap();
                    self.seq_handle.drain_output();
                }
                "P6" => {
                    let msg = Message::RPN14(Ch1, controlbase + 8, status as u16 / 2);
                    self.seq_port.send_message(&msg).unwrap();
                    self.seq_handle.drain_output();
                }

                "group_a" => {
                    maschine.set_midi_note_base(24);
                }
                "group_b" => {
                    maschine.set_midi_note_base(36);
                }
                "group_c" => {
                    maschine.set_midi_note_base(48);
                }
                "group_d" => {
                    maschine.set_midi_note_base(60);
                }
                "group_e" => {
                    maschine.set_midi_note_base(72);
                }
                "group_f" => {
                    maschine.set_midi_note_base(84);
                }
                "group_g" => {
                    maschine.set_midi_note_base(96);
                }
                "group_h" => {
                    maschine.set_midi_note_base(108);
                }
                _ => {}
            }
        }
        self.send_osc_msg(&*format!("/{}", button), osc_args![status as f32]);
    }   

    pub fn send_osc_encoder_msg(&self, delta: i32) {
        self.send_osc_msg("/maschine/encoder", osc_args![delta]);
    }
}

impl<'a> MaschineHandler for MHandler<'a> {
    fn pad_pressed(&mut self, maschine: &mut dyn Maschine, pad_idx: usize, pressure: f32) {
        // ...existing code...
    }

    fn pad_aftertouch(&mut self, maschine: &mut dyn Maschine, pad_idx: usize, pressure: f32) {
        // ...existing code...
    }

    fn pad_released(&mut self, maschine: &mut dyn Maschine, pad_idx: usize) {
        // ...existing code...
    }

    fn encoder_step(&mut self, _: &mut dyn Maschine, _: usize, delta: i32) {
        self.send_osc_encoder_msg(delta);
    }

    fn button_down(
        &mut self,
        maschine: &mut dyn Maschine,
        btn: MaschineButton,
        byte: u8,
        is_down: bool,
    ) {
        self.send_osc_button_msg(maschine, btn, byte as usize, is_down);
    }

    fn button_up(
        &mut self,
        maschine: &mut dyn Maschine,
        btn: MaschineButton,
        byte: u8,
        is_down: bool,
    ) {
        self.send_osc_button_msg(maschine, btn, byte as usize, is_down);
    }
}

pub fn ev_loop(device: &mut dyn Maschine, mhandler: &mut MHandler) {
    let mut fds = [
        PollFd::new(device.get_fd(), POLLIN, EventFlags::empty()),
        PollFd::new(mhandler.osc_socket.as_raw_fd(), POLLIN, EventFlags::empty()),
    ];

    let mut now = SystemTime::now();
    let mut now2 = SystemTime::now();
    let timer_interval = Duration::from_millis(16);
    let mut timer_interval2;
    let mut step = 0;
    let mut check = 0;
    let mut active = false;
    loop {
        poll(&mut fds, 16).unwrap();

        if fds[0].revents().unwrap().contains(POLLIN) {
            device.readable(mhandler);
        }

        if fds[1].revents().unwrap().contains(POLLIN) {
            mhandler.recv_osc_msg(device);
        }

        if now.elapsed().unwrap() >= timer_interval {
            device.write_lights();
            now = SystemTime::now();
        }
        if device.get_playing() == true {
            timer_interval2 = Duration::from_millis(device.get_seq_speed());
            active = true;
            if device.note_check(step) == 1 && now2.elapsed().unwrap() >= timer_interval2 && check == 0
            {
                let msg = device.load_notes(step, 1);
                mhandler.seq_port.send_message(&msg).unwrap();
                mhandler.seq_handle.drain_output();
                check = 1;
            };
            if now2.elapsed().unwrap() >= timer_interval2 * 2 && device.note_check(step) == 1 {
                let msg = device.load_notes(step, 0);
                mhandler.seq_port.send_message(&msg).unwrap();
                mhandler.seq_handle.drain_output();
                now2 = SystemTime::now();
                step += 1;
                check = 0;
            } else if now2.elapsed().unwrap() >= timer_interval2 * 2 && device.note_check(step) == 0 {
                step += 1;
                check = 0;
                now2 = SystemTime::now();
            };
            if step >= 16 {
                step = 0;
            };
        } else if active == true {
            let msg = device.load_notes(step, 0);
            mhandler.seq_port.send_message(&msg).unwrap();
            mhandler.seq_handle.drain_output();
            active = false;
        }
    }
}

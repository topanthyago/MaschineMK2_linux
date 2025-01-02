//  maschine.rs: user-space drivers for native instruments USB HIDs
//  Copyright (C) 2015 William Light <wrl@illest.net>
//
//  This program is free software: you can redistribute it and/or modify
//  it under the terms of the GNU Lesser General Public License as
//  published by the Free Software Foundation, either version 3 of the
//  License, or (at your option) any later version.
//
//  This program is distributed in the hope that it will be useful,
//  but WITHOUT ANY WARRANTY; without even the implied warranty of
//  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//  GNU Lesser General Public License for more details.
//
//  You should have received a copy of the GNU Lesser General Public
//  License along with this program.  If not, see
//  <http://www.gnu.org/licenses/>.

mod handler;
mod osc;
mod utils;

use std::env;
use std::path::Path;
use std::thread;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};

use std::time::{Duration, SystemTime};

extern crate nix;

extern crate hsl;
extern crate tinyosc;

use nix::fcntl::{O_NONBLOCK, O_RDWR};
use nix::{fcntl, sys};

extern crate alsa_seq;
extern crate midi;

use alsa_seq::*;
use midi::*;
// use devices::mk2::Mikro;
use handler::MHandler;

mod base;
mod devices;

use base::{maschine, Maschine, MaschineButton, MaschineHandler};
use utils::{usage, PAD_RELEASED_BRIGHTNESS};

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() < 2 {
        usage(&args[0]);
        panic!("missing hidraw device path");
    }

    let dev_fd = match fcntl::open(
        Path::new(&args[1]),
        O_RDWR | O_NONBLOCK,
        sys::stat::Mode::empty(),
    ) {
        Err(err) => panic!("couldn't open {}: {}", args[1], err.errno().desc()),
        Ok(file) => file,
    };

    let osc_socket = UdpSocket::bind("127.0.0.1:42434").unwrap();

    let seq_handle = SequencerHandle::open("maschine.rs", HandleOpenStreams::Output).unwrap();
    let seq_port = seq_handle
        .create_port(
            "Pads MIDI",
            PortCapabilities::PORT_CAPABILITY_READ | PortCapabilities::PORT_CAPABILITY_SUBS_READ,
            PortType::MidiGeneric,
        )
        .unwrap();

    let mut device = devices::mk2::Mikro::new(dev_fd);

    let mut handler = MHandler::new(&seq_handle, &seq_port, &osc_socket);

    device.clear_screen();

    //Trying to draw stuff here
    if args.len() < 3 {
        device.write_screen();
    } else {
        println!("RUNNING!")
    }
    //println!("{}", std::env::current_dir().unwrap().display());
    for i in 0..16 {
        device.set_pad_light(i, handler.pad_color(), PAD_RELEASED_BRIGHTNESS);
    }

    handler::ev_loop(&mut device, &mut handler);
}

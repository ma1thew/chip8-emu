extern crate minifb;
extern crate cpal;
extern crate rand;

use std::env;
use std::fs::File;
use std::io::Read;
use std::time::{Instant, Duration};
use std::thread;
use std::sync::mpsc;
use std::path::Path;

use minifb::{Window, WindowOptions, Key};
use cpal::traits::{HostTrait, DeviceTrait, StreamTrait};
use cpal::SampleFormat;

const PROGRAM_START_ADDR: usize = 0x200;
const FONT_START_ADDR: usize = 0x000;
const MEMORY_LENGTH: usize = 0x1000;
const SCREEN_HEIGHT: usize = 0x20;
const SCREEN_WIDTH: usize = 0x40;
const PIXEL_OFF_COLOR: u32 = 0x00000000;
const PIXEL_ON_COLOR: u32 = 0x00FFFFFF;
const FONT_BYTES_PER_CHAR: u16 = 5;
const FONT_DATA: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

struct Registers {
    pub i: u16,
    pub pc: u16,
    pub v: [u8; 0x10],
}

impl Registers {
    fn new() -> Self {
        Self {
            i: 0,
            pc: PROGRAM_START_ADDR as u16,
            v: [0; 0x10],
        }
    }
}

struct Display {
    framebuffer: Vec<u32>,
    window: Window,
    scale_factor: usize,
}

impl Display {
    fn new(scale_factor: usize) -> Self {
        Self {
            framebuffer: vec![PIXEL_OFF_COLOR; SCREEN_HEIGHT * SCREEN_WIDTH * scale_factor * scale_factor],
            window: Window::new("CHIP-8 Emulator", SCREEN_WIDTH * scale_factor, SCREEN_HEIGHT * scale_factor, WindowOptions::default()).unwrap(),
            scale_factor,
        }
    }

    fn clear(&mut self) {
        for pixel in &mut self.framebuffer {
            *pixel = PIXEL_OFF_COLOR;
        }
    }

    fn redraw(&mut self) {
        self.window.update_with_buffer(&self.framebuffer, SCREEN_WIDTH * self.scale_factor, SCREEN_HEIGHT * self.scale_factor).unwrap();
    }

    fn flip_pixel(&mut self, x: usize, y: usize) -> bool {
        if let Some(pixel) = self.get_pixel(x, y) {
            if *pixel == PIXEL_OFF_COLOR {
                self.set_pixel(x, y, PIXEL_ON_COLOR);
                false
            } else {
                self.set_pixel(x, y, PIXEL_OFF_COLOR);
                true
            }
        } else {
            false
        }
    }

    fn get_pixel(&self, x: usize, y: usize) -> Option<&u32> {
        self.framebuffer.get(y * self.scale_factor * self.scale_factor * SCREEN_WIDTH + x * self.scale_factor)
    }

    fn set_pixel(&mut self, x: usize, y: usize, color: u32) {
        for x_scaled in 0..self.scale_factor {
            for y_scaled in 0..self.scale_factor {
                //if let Some(pixel) = self.framebuffer.get_mut((y * self.scale_factor + y_scaled) * SCREEN_WIDTH + x * self.scale_factor + x_scaled) {
                if let Some(pixel) = self.framebuffer.get_mut((y * self.scale_factor + y_scaled) * self.scale_factor * SCREEN_WIDTH + x * self.scale_factor + x_scaled ) {
                    *pixel = color;
                }
            }
        }
    }

    fn is_key_down(&self, key: Key) -> bool {
        self.window.is_key_down(key)
    }

    fn is_open(&self) -> bool {
        self.window.is_open()
    }

    fn update_without_redraw(&mut self) {
        self.window.update();
    }
}

struct Buzzer {
    stream: cpal::Stream,
}

impl Buzzer {
    fn new() -> Self {
        let host = cpal::default_host();
        let device = host.devices().unwrap().next().unwrap();
        let supported_config = device.default_output_config().expect("Failed to determine default output configuration for audio device!");
        let sample_format = supported_config.sample_format();
        let config = supported_config.into();
        let stream = match sample_format {
            SampleFormat::F32 => Self::build_stream::<f32>(&device, &config),
            SampleFormat::I16 => Self::build_stream::<i16>(&device, &config),
            SampleFormat::U16 => Self::build_stream::<u16>(&device, &config),
        };
        stream.pause().unwrap();
        Self {
            stream,
        }
    }

    fn build_stream<T>(device: &cpal::Device, config: &cpal::StreamConfig) -> cpal::Stream
    where
        T: cpal::Sample
    {
        let sample_rate = config.sample_rate.0 as f32;
        let channels = config.channels as usize;

        let mut sample_clock = 0f32;
        let mut next_value = move || {
            sample_clock = (sample_clock + 1.0) % sample_rate;
            (sample_clock * 440.0 * 2.0 * std::f32::consts::PI / sample_rate).sin()
        };

        let err_fn = |err| println!("An error occurend on audio stream: {}", err);

        device.build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                Self::write_data(data, channels, &mut next_value)
            },
            err_fn,
        ).expect("Failed to build audio output stream!")
    }

    fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> f32)
    where
        T: cpal::Sample
    {
        for frame in output.chunks_mut(channels) {
            let value: T = cpal::Sample::from::<f32>(&next_sample());
            for sample in frame.iter_mut() {
                *sample = value;
            }
        }
    }

    fn play(&self) {
        self.stream.play().unwrap();
    }

    fn pause(&self) {
        self.stream.pause().unwrap();
    }
}

// TODO: play sound
fn main() {
    let mut args = env::args();
    args.next(); // Discard path to binary
    let mut memory = vec![0; MEMORY_LENGTH];
    memory[FONT_START_ADDR..(FONT_START_ADDR + FONT_DATA.len())].copy_from_slice(&FONT_DATA);
    let mut display = Display::new(args.next()
        .expect("Invalid arguments. Specify a scale factor.")
        .trim()
        .parse::<usize>()
        .expect("Only integer scale factors are supported."));
    let cycle_duration = Duration::from_secs(1) / args.next()
        .expect("Invalid arguments. Specify a clock speed.")
        .trim()
        .parse::<u32>()
        .expect("Only integer clock speeds are supported.");
    {
        let file_name = args.next().expect("Invalid arguments. Specify the path to the program.");
        let mut program = File::open(Path::new(&file_name)).expect(&format!("Failed to open: {}", file_name));
        program.read(&mut memory[PROGRAM_START_ADDR..MEMORY_LENGTH]).expect("Failed to read program into memory!");
    }
    let mut stack: Vec<u16> = Vec::new();
    let mut reg = Registers::new();
    let mut delay_timer: u8 = 0x00;
    let mut sound_timer: u8 = 0x00;
    let buzzer = Buzzer::new();
    let (timer_tx, timer_rx) = mpsc::channel::<()>();
    let (end_tx, end_rx) = mpsc::channel::<()>();
    thread::spawn(move || {
        let cycle_duration = Duration::from_secs(1) / 60;
        loop {
            let start_time = Instant::now();
            if let Ok(_) = end_rx.try_recv() {
                break;
            }
            timer_tx.send(()).expect("Failed to update timers!");
            thread::sleep(cycle_duration.saturating_sub(start_time.elapsed()));
        }
    });
    let mut waiting_for_key: Option<usize> = None;
    let keymap = [
        Key::X,
        Key::Key1,
        Key::Key2,
        Key::Key3,
        Key::Q,
        Key::W,
        Key::E,
        Key::A,
        Key::S,
        Key::D,
        Key::Z,
        Key::C,
        Key::Key4,
        Key::R,
        Key::F,
        Key::V,
    ];
    while !display.is_key_down(Key::Escape) && display.is_open() {
        let cycle_start_time = Instant::now();
        if let Some(register) = waiting_for_key {
            for i in 0..keymap.len() {
                if display.is_key_down(keymap[i]) {
                    reg.v[register] = i as u8;
                    waiting_for_key = None;
                    break;
                }
            }
            display.update_without_redraw();
            thread::sleep(cycle_duration.saturating_sub(cycle_start_time.elapsed()));
            continue;
        }
        let operation = u16::from_be_bytes([memory[reg.pc as usize], memory[(reg.pc + 1) as usize]]);
        reg.pc += 2;
        let high_nibble = (operation & 0xF000) >> 12;
        match high_nibble {
            0x00 => {
                match operation {
                    0x00E0 => {
                        display.clear();
                        display.redraw();
                    },
                    0x00EE => reg.pc = stack.pop().expect("Attempted to return with an empty stack!"),
                    _ => panic!("Not implemented: {:#x} (execute machine language subroutine)", operation),
                }
            },
            0x01 => reg.pc = operation & 0x0FFF,
            0x02 => {
                stack.push(reg.pc);
                reg.pc = operation & 0x0FFF;
            },
            0x03 => {
                let register = (operation & 0x0F00) >> 8;
                if reg.v[register as usize] == (operation & 0x00FF) as u8 {
                    reg.pc += 2;
                }
            },
            0x04 => {
                let register = (operation & 0x0F00) >> 8;
                if reg.v[register as usize] != (operation & 0x00FF) as u8 {
                    reg.pc += 2;
                }
            },
            0x05 => {
                let x = (operation & 0x0F00) >> 8;
                let y = (operation & 0x00F0) >> 4;
                if reg.v[x as usize] == reg.v[y as usize] {
                    reg.pc += 2;
                }
            },
            0x06 => {
                let register = (operation & 0x0F00) >> 8;
                let value = (operation & 0x00FF) as u8;
                reg.v[register as usize] = value;
            },
            0x07 => {
                let register = ((operation & 0x0F00) >> 8) as usize;
                let value = (operation & 0x00FF) as u8;
                reg.v[register] = reg.v[register].wrapping_add(value);
            },
            0x08 => {
                let x = ((operation & 0x0F00) >> 8) as usize;
                let y = ((operation & 0x00F0) >> 4) as usize;
                match operation & 0x000F {
                    0x00 => reg.v[x] = reg.v[y],
                    0x01 => reg.v[x] = reg.v[x] | reg.v[y],
                    0x02 => reg.v[x] = reg.v[x] & reg.v[y],
                    0x03 => reg.v[x] = reg.v[x] ^ reg.v[y],
                    0x04 => {
                        let (result, overflowed) = reg.v[x].overflowing_add(reg.v[y]);
                        reg.v[x] = result;
                        reg.v[0xF] = overflowed as u8;
                    },
                    0x05 => {
                        let (result, overflowed) = reg.v[x].overflowing_sub(reg.v[y]);
                        reg.v[x] = result;
                        reg.v[0xF] = (!overflowed) as u8;
                    },
                    0x06 => {
                        reg.v[x] = reg.v[y] >> 1;
                        reg.v[0xF] = reg.v[y] & 0x01;
                    },
                    0x07 => {
                        let (result, overflowed) = reg.v[y].overflowing_sub(reg.v[x]);
                        reg.v[x] = result;
                        reg.v[0xF] = (!overflowed) as u8;
                    }
                    0x0E => {
                        reg.v[x] = reg.v[y] << 1;
                        reg.v[0xF] = (reg.v[y] & 0x80) >> 7;
                    }
                    _ => panic!("Invalid instruction: {:#x}", operation),
                }
            },
            0x09 => {
                let x = (operation & 0x0F00) >> 8;
                let y = (operation & 0x00F0) >> 4;
                if reg.v[x as usize] != reg.v[y as usize] {
                    reg.pc += 2;
                }
            },
            0x0A => reg.i = operation & 0x0FFF,
            0x0B => reg.pc = (operation & 0x0FFF) + (reg.v[0] as u16),
            0x0C => {
                let register = ((operation & 0x0F00) >> 8) as usize;
                let mask = (operation & 0x00FF) as u8;
                reg.v[register] = rand::random::<u8>() & mask;
            },
            0x0D => {
                reg.v[0xF] = 0x00;
                let x = reg.v[((operation & 0x0F00) >> 8) as usize] % (SCREEN_WIDTH as u8);
                let y = reg.v[((operation & 0x00F0) >> 4) as usize] % (SCREEN_HEIGHT as u8);
                let rows = operation & 0x000F;
                for row in 0..rows {
                    let row_data = memory[(reg.i + row) as usize];
                    for column in 0..8 {
                        let pixel_state = (row_data << column) & 0x80;
                        if pixel_state == 0x80 {
                            reg.v[0xF] |= display.flip_pixel((x + column) as usize, (y + (row as u8)) as usize) as u8;
                        }
                    }
                }
                display.redraw();
            },
            0x0E => {
                let key = keymap[reg.v[((operation & 0x0F00) >> 8) as usize] as usize];
                match operation & 0x00FF {
                    0x9E => {
                        if display.is_key_down(key) {
                            reg.pc += 2;
                        }
                    },
                    0xA1 => {
                        if !display.is_key_down(key) {
                            reg.pc += 2;
                        }
                    },
                    _ => panic!("Invalid instruction: {:#x}", operation),
                }
            },
            0x0F => {
                let register = ((operation & 0x0F00) >> 8) as usize;
                match operation & 0x00FF {
                    0x07 => reg.v[register] = delay_timer,
                    0x0A => waiting_for_key = Some(register),
                    0x15 => delay_timer = reg.v[register],
                    0x18 => sound_timer = reg.v[register],
                    0x1E => reg.i = reg.i.wrapping_add(reg.v[register] as u16),
                    0x29 => reg.i = FONT_START_ADDR as u16 + reg.v[register] as u16 * FONT_BYTES_PER_CHAR,
                    0x33 => {
                        memory[reg.i as usize] = reg.v[register] / 100;
                        memory[reg.i as usize + 1] = (reg.v[register] % 100) / 10;
                        memory[reg.i as usize + 2] = reg.v[register] % 10;
                    },
                    0x55 => {
                        for i in 0..=register {
                            memory[reg.i as usize] = reg.v[i];
                            reg.i += 1;
                        }
                    },
                    0x65 => {
                        for i in 0..=register {
                            reg.v[i] = memory[reg.i as usize];
                            reg.i += 1;
                        }
                    },
                    _ => panic!("Invalid instruction: {:#x}", operation),
                }
            }
            _ => panic!("Not implemented: {:#x}", operation),
        }
        if let Ok(()) = timer_rx.try_recv() {
            delay_timer = delay_timer.saturating_sub(1);
            sound_timer = sound_timer.saturating_sub(1);
        }
        if sound_timer > 1 {
            buzzer.play();
        } else {
            buzzer.pause();
        }
        // saturating_sub is still unstable for durations
        /*if cycle_duration > cycle_start_time.elapsed() {
            thread::sleep(cycle_duration - cycle_start_time.elapsed());
        }*/
        thread::sleep(cycle_duration.saturating_sub(cycle_start_time.elapsed()));
    }
    end_tx.send(()).unwrap();
}

//! Feeds back the input stream directly into the output stream.
//!
//! Assumes that the input and output devices can use the same stream configuration and that they
//! support the f32 sample format.
//!
//! Uses a delay of `LATENCY_MS` milliseconds in case the default input and output streams are not
//! precisely synchronised.

use std::io::stdin;

use rand::thread_rng;
use rand_distr::{Distribution, Normal, NormalError};

use circular_queue::CircularQueue;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::RingBuffer;

use std::sync::mpsc::channel;
extern crate npy;

mod filter;
mod nlmf;
mod plot;
mod processing;
use processing::{Stereo2MonoCapture, Mono2StereoOutput, AECFiltering};

const LATENCY_MS: f32 = 200.0;

fn main() -> Result<(), anyhow::Error> {
    // get mu from command line
    let args: Vec<String> = std::env::args().collect();
    const DEFAULT_MU: f32 = 1.0;
    let mu: f32 = match args.len() {
        // one argument passed
        2 => match args[1].parse() {
            Ok(val) => val,
            _ => {
                eprintln!(
                    "Failed to parse mu value from command line; using default = {}.",
                    DEFAULT_MU
                );
                DEFAULT_MU
            }
        },
        _ => {
            eprintln!("No value of mu given; using default = {}.", DEFAULT_MU);
            DEFAULT_MU
        }
    };

    let host = cpal::default_host();

    // Default devices.
    let input_device = {
        host.devices()
            .expect("failed to get devices")
            .filter(|device| {
                device
                    .name()
                    .expect("failed to get name of device")
                    .contains("Mikrofon")
            })
            .next()
    }
    .expect("failed to get Mikrofon device");
    let capture_device = {
        host.devices()
            .expect("failed to get devices")
            .filter(|device| {
                device
                    .name()
                    .expect("failed to get name of device")
                    .contains("Stereomix")
            })
            .next()
    }
    .expect("failed to get stereomix device");
    let output_device = {
        host.devices()
            .expect("failed to get devices")
            .filter(|device| {
                device
                    .name()
                    .expect("failed to get name of device")
                    .contains("CABLE Input")
            })
            .next()
    }
    .expect("failed to get CABLE Input device");

    println!("Using input device: \"{}\"", input_device.name()?);
    println!(
        "Using stereomix output device: \"{}\"",
        capture_device.name()?
    );
    println!("Using Cable Output device: \"{}\"", output_device.name()?);


    // We'll try and use the same configuration between streams to keep it simple.
    /*
    let config: cpal::StreamConfig = cpal::StreamConfig {
        channels: 1,
        .. input_device.default_input_config()?.into()
    }; */
    let config: cpal::StreamConfig = input_device.default_input_config()?.into();

    // Create a delay in case the input and output devices aren't synced.
    let latency_frames = (LATENCY_MS / 1_000.0) * config.sample_rate.0 as f32;
    let latency_samples = latency_frames as usize; //* config.channels as usize;

    // The buffers to share samples
    let input_ring = RingBuffer::new(latency_samples * 2);
    let (mut input_ring_producer, mut input_ring_consumer) = input_ring.split();

    let capture_ring = RingBuffer::new(latency_samples * 2);
    let (mut capture_ring_producer, mut capture_ring_consumer) = capture_ring.split();

    let output_ring = RingBuffer::new(latency_samples * 2);
    let (mut output_ring_producer, mut output_ring_consumer) = output_ring.split();

    // Fill the samples with 0.0 equal to the length of the delay.
    for _ in 0..latency_samples {
        // The ring buffer has twice as much space as necessary to add latency here,
        // so this should never fail
        input_ring_producer.push(0.0).unwrap();
        capture_ring_producer.push(0.0).unwrap();
        output_ring_producer.push(0.0).unwrap();
    }

    let (s1, r1) = channel();

    let mut input_processing = Stereo2MonoCapture::new(input_ring_producer);
    let mut capture_processing = Stereo2MonoCapture::new(capture_ring_producer);
    let mut output_processing = Mono2StereoOutput::new(output_ring_consumer);
    let mut filter_processing = AECFiltering::new(input_ring_consumer, capture_ring_consumer, output_ring_producer, 1.0, Some(s1));

    // Build streams.
    println!(
        "Attempting to build streams with f32 samples and `{:?}`.",
        config
    );
    let input_stream = input_device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| input_processing.callback(data),
        err_fn,
    )?;
    println!("Succeded input stream");
    let capture_stream = capture_device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| capture_processing.callback(data),
        err_fn,
    )?;
    println!("Succeded capture stream");
    let output_stream = output_device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| output_processing.callback(data),
        err_fn,
    )?;
    println!("Succeded output stream");


    println!("Successfully built streams.");

    // Play the streams.
    println!("Starting the input and capture streams");

    capture_stream.play()?;
    input_stream.play()?;
    output_stream.play()?;

    let mut plotter = plot::Plotter::new(5.0, 0.0, 1.0, 128)?;
    println!("latency samples {}", latency_samples);
    // let mut plotter2 = plot::Plotter::new(2.0, -0.5,0.5, 65536)?;

    // Run for 3 seconds before closing.
    println!("Everything looks good! Press enter to exit...");
    //std::thread::sleep(std::time::Duration::from_secs(15));

    while !plotter.window.is_key_down(minifb::Key::Escape) {
        filter_processing.process(); // process data
        for val in r1.try_iter() {
            plotter.data.push(val);
        }
        plotter.tick()?;
    }
    drop(plotter);

    // let _ = stdin().read_line(&mut String::new());

    drop(input_stream);
    drop(capture_stream);
    drop(output_stream);
    /*
        let mic = r1.iter().collect::<Vec<f32>>();
        let capture = r2.iter().collect::<Vec<f32>>();
        let output = r3.iter().collect::<Vec<f32>>();

        npy::to_file("C:\\Users\\NaOH-de\\Documents\\Projects\\AEC/mic.npy", mic).unwrap();
        npy::to_file("C:\\Users\\NaOH-de\\Documents\\Projects\\AEC/capture.npy", capture).unwrap();
        npy::to_file("C:\\Users\\NaOH-de\\Documents\\Projects\\AEC/output.npy", output).unwrap();
    */
    println!("Done!");
    Ok(())
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("an error occurred on stream: {}", err);
}

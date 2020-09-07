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

const LATENCY_MS: f32 = 50.0;

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
    let latency_samples = latency_frames as usize * config.channels as usize;

    // The buffers to share samples
    let input_ring = RingBuffer::new(latency_samples * 2);
    let (mut input_ring_producer, mut input_ring_consumer) = input_ring.split();

    let capture_ring = RingBuffer::new(latency_samples * 2);
    let (mut capture_ring_producer, mut capture_ring_consumer) = capture_ring.split();

    //let output_ring = RingBuffer::new(latency_samples * 2);
    //let (mut output_ring_producer, mut output_ring_consumer) = output_ring.split();

    // Fill the samples with 0.0 equal to the length of the delay.
    for _ in 0..latency_samples {
        // The ring buffer has twice as much space as necessary to add latency here,
        // so this should never fail
        input_ring_producer.push(0.0).unwrap();
        capture_ring_producer.push(0.0).unwrap();
        // output_ring_producer.push(0.0).unwrap();
    }

    // debugging stuff:
    /*
     let (s1, r1) = channel();
     let (s2, r2) = channel();
     let (s3, r3) = channel();
    */
    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let mut output_fell_behind = false;
        let mut is_even_sample = true;
        let mut last_sample: f32 = 0.0;
        for &sample in data {
            if is_even_sample {
                is_even_sample = false;
                last_sample = sample;
            } else {
                let merged_sample = sample + last_sample;
                is_even_sample = true;
                if input_ring_producer.push(merged_sample).is_err() {
                    output_fell_behind = true;
                }
            }
            //s1.send(sample).unwrap();
        }
        if output_fell_behind {
            eprintln!("(mic:) output stream fell behind: try increasing latency");
        }
    };

    let capture_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let mut output_fell_behind = false;
        let mut is_even_sample = true;
        let mut last_sample: f32 = 0.0;
        for &sample in data {
            if is_even_sample {
                is_even_sample = false;
                last_sample = sample;
            } else {
                let merged_sample = sample + last_sample;
                is_even_sample = true;
                if capture_ring_producer.push(merged_sample).is_err() {
                    output_fell_behind = true;
                }
            }
            //s2.send(sample).unwrap();
        }

        if output_fell_behind {
            eprintln!("(capture:) output stream fell behind: try increasing latency");
        }
    };

    let mut filter_buffer = CircularQueue::with_capacity(1024);
    for _ in 0..1024 {
        filter_buffer.push(0.0);
    }
    let weights: Vec<f32> = {
        let mut rng = thread_rng();
        let normal = Normal::new(0.0, 0.5)?;
        normal
            .sample_iter(&mut rng)
            .take(1024)
            .collect::<Vec<f32>>()
    };
    let mut filter: nlmf::NLMF<f32> = nlmf::NLMF::new(1024, mu, 1.0, weights);
    let mut lowpass_filter = filter::Filter::new(filter::LowPass(2500.0));

    let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let mut input_fell_behind = None;
        let mut is_odd_sample = false;
        let mut last_sample: f32 = 0.0;
        for sample in data {
            if is_odd_sample {
                *sample = last_sample;
                is_odd_sample = false;
            } else {
                let mic_sample = match input_ring_consumer.pop() {
                    Ok(s) => s,
                    Err(err) => {
                        input_fell_behind = Some(err);
                        0.0
                    }
                };
                let capture_sample = match capture_ring_consumer.pop() {
                    Ok(s) => s,
                    Err(err) => {
                        input_fell_behind = Some(err);
                        0.0
                    }
                };
                filter_buffer.push(capture_sample);
                let mut filter_input = filter_buffer
                    .asc_iter()
                    .map(|&val| val)
                    .collect::<Vec<f32>>();
                // filter_input.push(1.0);
                let aec_output = filter.adapt(&filter_input, mic_sample);
                let filtered = lowpass_filter.tick(mic_sample - aec_output);
                *sample = filtered;
                last_sample = *sample;
                is_odd_sample = true;

                //s3.send(last_sample).unwrap();
                //println!("Weights: {:?}", filter_buffer);
            }
        }
        if let Some(err) = input_fell_behind {
            eprintln!(
                "input stream fell behind: {:?}: try increasing latency",
                err
            );
        }
    };

    // Build streams.
    println!(
        "Attempting to build streams with f32 samples and `{:?}`.",
        config
    );
    let input_stream = input_device.build_input_stream(&config, input_data_fn, err_fn)?;
    println!("Succeded input stream");
    let capture_stream = capture_device.build_input_stream(&config, capture_data_fn, err_fn)?;
    println!("Succeded capture stream");
    let output_stream = output_device.build_output_stream(&config, output_data_fn, err_fn)?;
    println!("Succeded output stream");

    println!("Successfully built streams.");

    // Play the streams.
    println!("Starting the input and capture streams");

    capture_stream.play()?;
    input_stream.play()?;
    output_stream.play()?;

    // Run for 3 seconds before closing.
    println!("Everything looks good! Press enter to exit...");
    //std::thread::sleep(std::time::Duration::from_secs(15));
    let _ = stdin().read_line(&mut String::new());

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

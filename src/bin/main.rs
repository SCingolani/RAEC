//! Feeds back the input stream directly into the output stream.
//!
//! Assumes that the input and output devices can use the same stream configuration and that they
//! support the f32 sample format.
//!
//! Uses a delay of `LATENCY_MS` milliseconds in case the default input and output streams are not
//! precisely synchronised.

use clap::{App, Arg};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use processing::{AECFiltering, Mono2StereoOutput, Stereo2MonoCapture};
use raec::*;
use ringbuf::RingBuffer;
use std::io::stdin;
use std::sync::{Arc, Mutex};
use std::thread::Thread;

const LATENCY_MS: f32 = 100.0;

fn list_devices() -> Result<(), anyhow::Error> {
    // Adapted from https://github.com/RustAudio/cpal/blob/269c60fde0c1c09fbdf50d65d7bf0d3a4e8d217c/examples/enumerate.rs
    println!("Supported hosts:\n  {:?}", cpal::ALL_HOSTS);
    let available_hosts = cpal::available_hosts();
    println!("Available hosts:\n  {:?}", available_hosts);

    for host_id in available_hosts {
        println!("{}", host_id.name());
        let host = cpal::host_from_id(host_id)?;

        let default_in = host.default_input_device().map(|e| e.name().unwrap());
        let default_out = host.default_output_device().map(|e| e.name().unwrap());
        println!("  Default Input Device:\n    {:?}", default_in);
        println!("  Default Output Device:\n    {:?}", default_out);

        let devices = host.devices()?;
        println!("  Devices: ");
        for (device_index, device) in devices.enumerate() {
            println!("  {}. \"{}\"", device_index, device.name()?);

            // Input configs
            if let Ok(conf) = device.default_input_config() {
                println!("    Default input stream config:\n      {:?}", conf);
            }
            let input_configs = match device.supported_input_configs() {
                Ok(f) => f.collect(),
                Err(e) => {
                    println!("    Error getting supported input configs: {:?}", e);
                    Vec::new()
                }
            };
            if !input_configs.is_empty() {
                println!("    All supported input stream configs:");
                for (config_index, config) in input_configs.into_iter().enumerate() {
                    println!("      {}.{}. {:?}", device_index, config_index, config);
                }
            }

            // Output configs
            if let Ok(conf) = device.default_output_config() {
                println!("    Default output stream config:\n      {:?}", conf);
            }
            let output_configs = match device.supported_output_configs() {
                Ok(f) => f.collect(),
                Err(e) => {
                    println!("    Error getting supported output configs: {:?}", e);
                    Vec::new()
                }
            };
            if !output_configs.is_empty() {
                println!("    All supported output stream configs:");
                for (config_index, config) in output_configs.into_iter().enumerate() {
                    println!("      {}.{}. {:?}", device_index, config_index, config);
                }
            }
        }
    }

    Ok(())
}

fn main() -> Result<(), anyhow::Error> {
    // Parse CLI arguments
    let matches = App::new("RAEC")
        .version("0.1")
        .author("")
        .about("Simple acoustic echo cancellation experiment.")
        .arg(
            Arg::with_name("host_id")
                .long("host")
                .value_name("HOST_ID")
                .help("Sets the audio host to use")
                .takes_value(true), //.group("device_ids"),
        )
        .arg(
            Arg::with_name("mic_device_id")
                .short("m")
                .long("microphone")
                .value_name("MICROPHONE_DEVICE_ID")
                .help("Sets which device to use as the microphone signal")
                .takes_value(true), //.group("device_ids"),
        )
        .arg(
            Arg::with_name("capture_device_id")
                .short("c")
                .long("capture")
                .value_name("CAPTURE_DEVICE_ID")
                .help("Sets which device to use as the capture signal")
                .takes_value(true), //.group("device_ids"),
        )
        .arg(
            Arg::with_name("output_device_id")
                .short("o")
                .long("output")
                .value_name("OUTPUT_DEVICE_ID")
                .help("Sets which device to use as the output signal")
                .takes_value(true), //.group("device_ids"),
        )
        .arg(
            Arg::with_name("list_devices")
                .short("l")
                .long("list")
                .help("List available audio devices and their IDs"),
        )
        .arg(
            Arg::with_name("mu")
                .long("mu")
                .default_value("1.0")
                .help("Adaptive filter step size"),
        )
        .get_matches();

    if matches.is_present("list_devices") {
        return list_devices();
    }

    assert!(
        matches.is_present("host_id") &&
        matches.is_present("mic_device_id") &&
        matches.is_present("capture_device_id") &&
        matches.is_present("output_device_id") ,
        "You must provide the IDs of the devices to use as well as the name of the audio host. See raec --help."
    );

    let mu = matches
        .value_of("mu")
        .unwrap() // SAFETY: "mu" has a default value
        .parse()
        .expect("Could not parse the value of mu");

    let host_id = matches.value_of("host_id").unwrap(); // SAFETY: we already checked that the group of IDs is present
    let host = cpal::host_from_id(
        cpal::available_hosts()
            .into_iter()
            .find(|id| id.name() == host_id)
            .expect(format!("Could not find the host \"{}\"", host_id).as_str()),
    )?;

    // Devices
    // Microphone:
    let input_device =
        host.devices()?
        .nth(matches.value_of("mic_device_id").unwrap()// SAFETY: We have checked already that the id is present in the arguments
            .parse()
            .expect("Failed to parse microphone ID; use raec --list to see a list of devices and use the corresponding number of the wished entry on the list as the ID.")
        )
        .expect("Failed to open microphone device");
    // Capture:
    let capture_device =
        host.devices()?
        .nth(matches.value_of("capture_device_id").unwrap()// SAFETY: We have checked already that the id is present in the arguments
            .parse()
            .expect("Failed to parse capture ID; use raec --list to see a list of devices and use the corresponding number of the wished entry on the list as the ID.")
        )
        .expect("Failed to open capture device");
    let output_device =
        host.devices()?
        .nth(matches.value_of("output_device_id").unwrap()// SAFETY: We have checked already that the id is present in the arguments
            .parse()
            .expect("Failed to parse output ID; use raec --list to see a list of devices and use the corresponding number of the wished entry on the list as the ID.")
        )
        .expect("Failed to open output device");

    println!("Using input device: \"{}\"", input_device.name()?);
    println!("Using capture device: \"{}\"", capture_device.name()?);
    println!("Using output device: \"{}\"", output_device.name()?);

    // We'll try and use the same configuration between streams to keep it simple.
    let config: cpal::StreamConfig = input_device.default_input_config()?.into();

    // Create a delay in case the input and output devices aren't synced.
    let latency_frames = (LATENCY_MS / 1_000.0) * config.sample_rate.0 as f32;
    let latency_samples = latency_frames as usize; //* config.channels as usize;

    // The buffers to share samples
    let input_ring = RingBuffer::new(latency_samples * 2);
    let (mut input_ring_producer, input_ring_consumer) = input_ring.split();

    let capture_ring = RingBuffer::new(latency_samples * 2);
    let (mut capture_ring_producer, capture_ring_consumer) = capture_ring.split();

    let output_ring = RingBuffer::new(latency_samples * 2);
    let (mut output_ring_producer, output_ring_consumer) = output_ring.split();

    // Fill the samples with 0.0 equal to the length of the delay.
    for _ in 0..latency_samples {
        // The ring buffer has twice as much space as necessary to add latency here,
        // so this should never fail
        input_ring_producer.push(0.0).unwrap();
        capture_ring_producer.push(0.0).unwrap();
        output_ring_producer.push(0.0).unwrap();
    }

    let shared_parking_thread_handle: Arc<Mutex<Option<Thread>>> = Arc::new(Mutex::new(None));

    let mut input_processing = Stereo2MonoCapture::new_with_parking(
        input_ring_producer,
        shared_parking_thread_handle.clone(),
    );
    let mut capture_processing = Stereo2MonoCapture::new(capture_ring_producer);
    let mut output_processing = Mono2StereoOutput::new(output_ring_consumer);
    let filter_processing = AECFiltering::new(
        input_ring_consumer,
        capture_ring_consumer,
        output_ring_producer,
        mu,
    );

    // Build streams.
    println!(
        "Attempting to build streams with f32 samples and `{:?}`.",
        config
    );
    let input_stream = input_device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| input_processing.callback_and_unpark(data),
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

    println!("latency samples {}", latency_samples);

    // filter_processing.debug_channel = Some(plot_send);

    let (processing_thread, parking_thread_handle) = filter_processing.start_thread();
    *shared_parking_thread_handle.lock().unwrap() = Some(parking_thread_handle);

    println!("Everything looks good! Press enter to exit...");

    let _ = stdin().read_line(&mut String::new());

    let _ = processing_thread.kill();

    drop(input_stream);
    drop(capture_stream);
    drop(output_stream);

    println!("Done!");
    Ok(())
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("an error occurred on stream: {}", err);
}

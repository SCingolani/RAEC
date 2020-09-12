use circular_queue::CircularQueue;
use rand::thread_rng;
use rand_distr::{Distribution, Normal};

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::Thread;

use crate::filter;
use crate::nlmf;

pub struct Stereo2MonoCapture {
    output_buffer: ringbuf::Producer<f32>,
    parked_thread: Option<Arc<Mutex<Option<Thread>>>>,
}

impl Stereo2MonoCapture {
    // trivial constructor
    pub fn new(buffer: ringbuf::Producer<f32>) -> Self {
        Stereo2MonoCapture {
            output_buffer: buffer,
            parked_thread: None,
        }
    }

    pub fn new_with_parking(
        buffer: ringbuf::Producer<f32>,
        parked_thread: Arc<Mutex<Option<Thread>>>,
    ) -> Self {
        Stereo2MonoCapture {
            output_buffer: buffer,
            parked_thread: Some(parked_thread),
        }
    }

    pub fn callback(&mut self, data: &[f32]) {
        let mut output_fell_behind = false;
        // iterate over couple of values
        for (input_l, input_r) in data.iter().step_by(2).zip(data.iter().step_by(2).skip(1)) {
            let merged_sample = 0.5 * (input_l + input_r);
            if self.output_buffer.push(merged_sample).is_err() {
                output_fell_behind = true;
            }
        }
        if output_fell_behind {
            eprintln!("(capture) output stream fell behind: try increasing latency");
        }
    }

    pub fn callback_and_unpark(&mut self, data: &[f32]) {
        let mut output_fell_behind = false;
        // iterate over couple of values
        for (input_l, input_r) in data.iter().step_by(2).zip(data.iter().step_by(2).skip(1)) {
            let merged_sample = 0.5 * (input_l + input_r);
            if self.output_buffer.push(merged_sample).is_err() {
                output_fell_behind = true;
            }
        }
        if output_fell_behind {
            eprintln!("(capture) output stream fell behind: try increasing latency");
        }
        let parked_thread_handle_lock = self.parked_thread.as_ref().unwrap().try_lock();
        if let Ok(maybe_parked_thread_handle) = parked_thread_handle_lock {
            if let Some(parked_thread_handle) = maybe_parked_thread_handle.as_ref() {
                parked_thread_handle.unpark();
            }
        }
    }
}

pub struct Mono2StereoOutput {
    input_buffer: ringbuf::Consumer<f32>,
}

impl Mono2StereoOutput {
    // trivial constructor
    pub fn new(buffer: ringbuf::Consumer<f32>) -> Self {
        Mono2StereoOutput {
            input_buffer: buffer,
        }
    }

    pub fn callback(&mut self, data: &mut [f32]) {
        let mut input_fell_behind = false;

        // variables to replicate input to generate stereo from mono:
        let mut flag = false;
        let mut last_sample = 0.0_f32;

        // iterate over samples to output
        for sample in data {
            let input: f32 = if !flag {
                flag = true;
                match self.input_buffer.pop() {
                    Ok(s) => {
                        last_sample = s;
                        s
                    }
                    Err(err) => {
                        input_fell_behind = true;
                        0.0
                    }
                }
            } else {
                flag = false;
                last_sample
            };
            *sample = input;
        }

        if input_fell_behind {
            eprintln!("(output) input stream fell behind: try increasing latency");
        }
    }
}

/// Struct to hold information of an instance of AECFiltering.
/// Such an object takes ownership of the buffers involved.
pub struct AECFiltering {
    /// Incoming buffer of microphone data
    mic_buffer: ringbuf::Consumer<f32>,
    /// Incoming buffer of reference data
    capture_buffer: ringbuf::Consumer<f32>,
    /// Outgoing buffer for output
    output_buffer: ringbuf::Producer<f32>,
    /// The adaptive FIR filter instance
    nlmf_filter: nlmf::NLMF<f32>,
    /// The running convolution to input into the FIR filter
    filter_buffer: CircularQueue<f32>,
    /// A low pass filter
    lowpass_filter: filter::Filter,
    /// A high pass filter
    highpass_fiter: filter::Filter,
    /// Control signal to kill the processing thread
    signal_channel: Option<mpsc::Receiver<()>>,
    /// Debug channel to communicate out the filling state of the buffers
    /// Message is (time (s), microphone buffer usage level (%), reference buffer usage level (%),
    /// output buffer usage level (%)): (f32, f32, f32, f32)
    pub debug_channel: Option<mpsc::Sender<(f32, f32, f32, f32)>>,
    /// Used for debugging with debug channel
    start_time: std::time::Instant,
}

/// When the thread to run the filter starts the AECFiltering struct is consumed.
/// This struct contains the thread handle and kill signal channel to be able to stop the filter.
pub struct RunningAECFiltering {
    kill_signal_sender: mpsc::Sender<()>,
    thread_join_handle: std::thread::JoinHandle<AECFiltering>,
}

impl RunningAECFiltering {
    fn new(
        kill_signal_sender: mpsc::Sender<()>,
        thread_join_handle: std::thread::JoinHandle<AECFiltering>,
    ) -> Self {
        let thread = thread_join_handle.thread();
        RunningAECFiltering {
            kill_signal_sender,
            thread_join_handle,
        }
    }

    /// kill the thread and consume the struct in the process
    pub fn kill(self) -> AECFiltering {
        self.kill_signal_sender.send(()).unwrap();
        self.thread_join_handle.join().unwrap() // may panic if the thread panicked
    }
}

impl AECFiltering {
    // hard-coded constructor; in the future parameterize this
    pub fn new(
        mic_buffer: ringbuf::Consumer<f32>,
        capture_buffer: ringbuf::Consumer<f32>,
        output_buffer: ringbuf::Producer<f32>,
        mu: f32,
    ) -> Self {
        let weights: Vec<f32> = {
            let mut rng = thread_rng();
            let normal = Normal::new(0.0, 0.5).unwrap();
            normal
                .sample_iter(&mut rng)
                .take(2048)
                .collect::<Vec<f32>>()
        };
        let nlmf_filter: nlmf::NLMF<f32> = nlmf::NLMF::new(2048, mu, 1.0, weights);
        let lowpass_filter = filter::Filter::new(filter::LowPass(3400.0));
        let highpass_fiter = filter::Filter::new(filter::HighPass(300.0));
        let mut filter_buffer = CircularQueue::with_capacity(2048);
        for _ in 0..2048 {
            filter_buffer.push(0.0);
        }
        AECFiltering {
            mic_buffer,
            capture_buffer,
            output_buffer,
            nlmf_filter,
            filter_buffer,
            lowpass_filter,
            highpass_fiter,
            signal_channel: None,
            debug_channel: None,
            start_time: std::time::Instant::now(),
        }
    }

    /// Starts the processing thread; will block until the thread starts and reports back its handle for unparking.
    pub fn start_thread(mut self) -> (RunningAECFiltering, Thread) {
        let (signal_sender, signal_receiver) = mpsc::channel();
        self.signal_channel = Some(signal_receiver);
        let thread_handle = Arc::new(Mutex::new(None));
        let thread_handle_clone = thread_handle.clone();
        let thread_joinhandle = std::thread::spawn(move || {
            {
                let mut shared_thread_handle_ref = thread_handle_clone.lock().unwrap();
                *shared_thread_handle_ref = Some(std::thread::current());
            }
            self.process()
        });
        // spinlock until we get the started thread handle
        while thread_handle.lock().unwrap().is_none() {
            std::thread::yield_now()
        }
        let the_handle = thread_handle.lock().unwrap().take().unwrap();
        (
            RunningAECFiltering::new(signal_sender, thread_joinhandle),
            the_handle,
        )
    }

    // process all available data in input buffers
    fn process(mut self) -> Self {
        loop {
            let signal = self.signal_channel.as_ref().unwrap().try_recv(); // here we unwrap because the thread starter has set this channel.
            match signal {
                Err(mpsc::TryRecvError::Disconnected) => {
                    eprintln!("Processing thread was disconnected without notice");
                    break;
                }
                Ok(()) => {
                    eprintln!("Processing thread received kill signal");
                    break;
                }
                _ => (),
            }
            let mut counter = 0;

            // as long as there is data in *both* buffers
            while !self.mic_buffer.is_empty()
                && !self.capture_buffer.is_empty()
                && !self.output_buffer.is_full()
            {
                // we are guaranteed there is data here as there can be only one consumer at a time
                let mic_sample = self.mic_buffer.pop().unwrap(); // see comment above to justify unwrap.
                let capture_sample = self.capture_buffer.pop().unwrap(); // see comment above to justify unwrap.
                                                                         // probably very inneficient:
                self.filter_buffer.push(capture_sample);
                let mut filter_input = self
                    .filter_buffer
                    .iter()
                    .map(|&val| val) // horrible
                    .collect::<Vec<f32>>();
                let (aec_output, novelty) =
                    self.nlmf_filter.adapt(&filter_input, mic_sample, 0.0025);
                let filtered = self
                    .highpass_fiter
                    .tick(self.lowpass_filter.tick(mic_sample - aec_output));

                if counter % 1_000 == 0 {
                    counter = 0;
                    match &self.debug_channel {
                        Some(ch) => ch
                            .send((
                                self.start_time.elapsed().as_secs_f32(),
                                self.mic_buffer.len() as f32 / self.mic_buffer.capacity() as f32,
                                self.capture_buffer.len() as f32
                                    / self.capture_buffer.capacity() as f32,
                                self.output_buffer.len() as f32
                                    / self.output_buffer.capacity() as f32,
                                //novelty * 100.,
                            ))
                            .unwrap(),
                        None => (),
                    };
                }
                counter += 1;

                // if we can no longer push to output buffer:
                if self.output_buffer.push(filtered).is_err() {
                    eprintln!("(filter) output stream fell behind: try increasing latency");
                    // no longer process elements!
                    break;
                }
            }
            // if by the time we are done the output buffer is getting very empty; fill it with zeros :/
            if (self.output_buffer.len() as f32 / self.output_buffer.capacity() as f32) < 0.2 {
                for _ in 0..self.output_buffer.capacity() / 2 {
                    self.output_buffer.push(0.0);
                }
                eprintln!("(filter) output buffer getting empty; i.e. inputs are too slow. filling with zeroes");
            }
            std::thread::park();
        }
        self
    }
}

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

use aec::nlmf;
use aec::processing::{Mono2StereoOutput, Stereo2MonoCapture};

pub fn callbacks_benchmark(c: &mut Criterion) {
    let input_ring = ringbuf::RingBuffer::<f32>::new(1024);
    let (input_ring_producer, mut input_ring_consumer) = input_ring.split();
    let output_ring = ringbuf::RingBuffer::<f32>::new(1024);
    let (mut output_ring_producer, output_ring_consumer) = output_ring.split();

    let mut input_processing = Stereo2MonoCapture::new(input_ring_producer);
    let mut output_processing = Mono2StereoOutput::new(output_ring_consumer);

    let bytes: &[f32] = &[0.0; 960];
    let mut_bytes: &mut [f32] = &mut [0.0; 960];

    let mut group = c.benchmark_group("callbacks");
    group.throughput(Throughput::Elements(bytes.len() as u64));
    group.bench_function("input process", |b| {
        while let Ok(_) = input_ring_consumer.pop() {}
        b.iter(|| {
            black_box(input_processing.callback(bytes));
            while let Ok(_) = input_ring_consumer.pop() {}
        })
    });
    group.bench_function("output process", |b| {
        while let Ok(_) = output_ring_producer.push(0.0) {}
        b.iter(|| {
            black_box(output_processing.callback(mut_bytes));
            while let Ok(_) = output_ring_producer.push(0.0) {}
        })
    });
    group.finish();
}

pub fn filter_adapt_benchmark(c: &mut Criterion) {
    let weights: [f32; nlmf::N_TAPS] = [0.0; nlmf::N_TAPS];

    let mut nlmf_filter: nlmf::NLMF<f32> = nlmf::NLMF::new(nlmf::N_TAPS, 1.0, 1.0, weights);

    let mut group = c.benchmark_group("Filter");
    group.throughput(Throughput::Elements(1 as u64));
    group.bench_function("nlmf.adapt", |b| {
        b.iter(|| black_box(nlmf_filter.adapt(0.0, 0.0, -1.0)))
    });
    group.bench_function("nlmf.adapt_no_update", |b| {
        b.iter(|| black_box(nlmf_filter.adapt(0.0, 0.0, f32::MAX)))
    });
}

criterion_group!(callbacks, callbacks_benchmark);
criterion_group!(filter, filter_adapt_benchmark);
criterion_main!(callbacks, filter);

use std::{thread, time::Duration};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() {
    let mut reader = hound::WavReader::new(include_bytes!("stereo-test/stereo-test.wav").as_ref())
        .expect("Failed to read WAV file");

    // get metadata from the WAV file
    let hound::WavSpec {
        sample_rate,
        sample_format,
        bits_per_sample,
        channels,
        ..
    } = reader.spec();
    let length_samples = reader.duration();
    let length_seconds = length_samples as f32 / sample_rate as f32;

    // this example assumes the sound has two channels
    assert_eq!(channels, 2);

    // convert the WAV data to floating point samples
    // e.g. i8 data is converted from [-128, 127] to [-1.0, 1.0]
    let samples_result: Result<Vec<f32>, _> = match sample_format {
        hound::SampleFormat::Int => {
            let max_value = 2_u32.pow(bits_per_sample as u32 - 1) - 1;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|sample| sample as f32 / max_value as f32))
                .collect()
        }
        hound::SampleFormat::Float => reader.samples::<f32>().collect(),
    };
    let mut samples = samples_result.unwrap();

    // channels are interleaved, so we put them together
    let sound_frames = oddio::Frames::from_iter(
        sample_rate,
        oddio::frame_stereo(&mut samples).iter().copied(),
    );

    // the rest is about the same as in the simple example
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");

    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let (mut mixer_handle, mixer) = oddio::split(oddio::Mixer::new());

    let stream = device
        .build_output_stream(
            &config,
            move |out_flat: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let out_stereo = oddio::frame_stereo(out_flat);
                oddio::run(&mixer, sample_rate, out_stereo);
            },
            move |err| {
                eprintln!("{}", err);
            },
        )
        .unwrap();
    stream.play().unwrap();

    mixer_handle
        .control::<oddio::Mixer<_>, _>()
        .play(oddio::FramesSignal::from(sound_frames));

    thread::sleep(Duration::from_secs(length_seconds.ceil() as u64));
}

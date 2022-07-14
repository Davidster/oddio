use std::{thread, time::Duration};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() {
    let mut reader = hound::WavReader::new(include_bytes!("stereo-test/stereo-test.wav").as_ref())
        .expect("Failed to read WAV file");
    let duration = reader.duration();
    let hound::WavSpec {
        sample_rate,
        sample_format,
        bits_per_sample,
        channels,
        ..
    } = reader.spec();
    let length_seconds = duration as f32 / sample_rate as f32;
    assert_eq!(channels, 2);

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
    let samples = samples_result.unwrap();

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");

    let track_data: Vec<_> = (0..duration as usize)
        .map(|i| [samples[i * 2], samples[i * 2 + 1]])
        .collect();

    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let track_frames = oddio::Frames::from_iter(config.sample_rate.0, track_data);

    let (mut mixer_handle, mixer) = oddio::split(oddio::Mixer::new());

    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let frames = oddio::frame_stereo(data);
                oddio::run(&mixer, sample_rate, frames);
            },
            move |err| {
                eprintln!("{}", err);
            },
        )
        .unwrap();
    stream.play().unwrap();

    mixer_handle
        .control::<oddio::Mixer<_>, _>()
        .play(oddio::FramesSignal::from(track_frames));

    thread::sleep(Duration::from_secs(length_seconds.ceil() as u64));
}

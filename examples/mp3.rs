use std::convert::TryFrom;
use std::{thread, time::Duration};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");

    let mut decoder = minimp3::Decoder::new(include_bytes!("stereo-test/stereo-test.mp3").as_ref());
    let mut mp3_frames: Vec<minimp3::Frame> = Vec::new();
    loop {
        match decoder.next_frame() {
            Ok(frame) => {
                mp3_frames.push(frame);
            }
            Err(minimp3::Error::Eof) => {
                break;
            }
            Err(err) => {
                eprintln!("{}", err);
                break;
            }
        }
    }
    let sample_rate_i32 = mp3_frames[0].sample_rate;
    let sample_rate = u32::try_from(mp3_frames[0].sample_rate).unwrap();
    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };
    let mut track_data: Vec<[f32; 2]> = Vec::new();
    for mp3_frame in mp3_frames {
        assert_eq!(mp3_frame.channels, 2);
        assert_eq!(mp3_frame.sample_rate, sample_rate_i32);
        for chunk in mp3_frame.data.chunks(2) {
            track_data.push([
                chunk[0] as f32 / std::i16::MAX as f32,
                chunk[1] as f32 / std::i16::MAX as f32,
            ]);
        }
    }
    let length_seconds = track_data.len() as f32 / sample_rate as f32;
    let mp3_oddio_frames = oddio::Frames::from_iter(config.sample_rate.0, track_data);

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
        .play(oddio::FramesSignal::from(mp3_oddio_frames));

    thread::sleep(Duration::from_secs(length_seconds.ceil() as u64));
}

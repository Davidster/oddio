use std::{thread, time::Duration};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() {
    // get device's sample rate
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");
    let device_sample_rate = device.default_output_config().unwrap().sample_rate().0;

    // get metadata from the WAV file
    // note that this wav file has a low sample rate so the sound quality is bad
    let mut reader = hound::WavReader::new(include_bytes!("stereo-test/stereo-test.wav").as_ref())
        .expect("Failed to read WAV file");
    let hound::WavSpec {
        sample_rate: source_sample_rate,
        sample_format,
        bits_per_sample,
        channels,
        ..
    } = reader.spec();
    let length_samples = reader.duration();
    let length_seconds = length_samples as f32 / source_sample_rate as f32;

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
    let mut samples_stereo: Vec<_> = oddio::frame_stereo(&mut samples).to_vec();

    if device_sample_rate != source_sample_rate {
        // resample the sound to the device sample rate using linear interpolation
        let old_sample_count = samples_stereo.len();
        let new_sample_count = (length_seconds * device_sample_rate as f32).ceil() as usize;
        let new_samples: Vec<_> = (1..(new_sample_count + 1))
            .map(|new_sample_number| {
                let old_sample_number = new_sample_number as f32
                    * (source_sample_rate as f32 / device_sample_rate as f32);

                // get the indices of the two samples that surround the old sample number
                let left_index = old_sample_number
                    .clamp(1.0, old_sample_count as f32)
                    .floor() as usize
                    - 1;
                let right_index = (left_index + 1).min(old_sample_count - 1);

                let left_sample = samples_stereo[left_index];
                if left_index == right_index {
                    [left_sample[0], left_sample[1]]
                } else {
                    let right_sample = samples_stereo[right_index];
                    let t = old_sample_number % 1.0;
                    [
                        (1.0 - t) * left_sample[0] + t * right_sample[0],
                        (1.0 - t) * left_sample[1] + t * right_sample[1],
                    ]
                }
            })
            .collect();
        samples_stereo = new_samples;
    }

    // channels are interleaved, so we put them together
    let sound_frames = oddio::Frames::from_iter(device_sample_rate, samples_stereo);

    let (mut mixer_handle, mixer) = oddio::split(oddio::Mixer::new());

    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(device_sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = device
        .build_output_stream(
            &config,
            move |out_flat: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let out_stereo = oddio::frame_stereo(out_flat);
                oddio::run(&mixer, device_sample_rate, out_stereo);
            },
            move |err| {
                eprintln!("{}", err);
            },
        )
        .unwrap();
    stream.play().unwrap();

    let yo = mixer_handle
        .control::<oddio::Mixer<_>, _>()
        .play(oddio::FramesSignal::from(sound_frames));

    thread::sleep(Duration::from_secs(length_seconds.ceil() as u64));
}

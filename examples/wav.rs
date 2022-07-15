use std::{thread, time::Duration};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");
    let device_sample_rate = device.default_output_config().unwrap().sample_rate();

    let mut reader = hound::WavReader::new(include_bytes!("stereo-test/stereo-test.wav").as_ref())
        .expect("Failed to read WAV file");

    // get metadata from the WAV file
    let hound::WavSpec {
        sample_rate: source_sample_rate,
        sample_format,
        bits_per_sample,
        channels,
        ..
    } = reader.spec();
    let length_samples = reader.duration();
    let length_seconds = length_samples as f32 / source_sample_rate as f32;

    dbg!(device_sample_rate, source_sample_rate);

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

    if device_sample_rate.0 != source_sample_rate {
        use dasp::interpolate::linear::Linear;
        use dasp::interpolate::sinc::Sinc;
        use dasp::signal::interpolate::Converter;
        use dasp::Signal;

        let interpolator = {
            // sinc interpolation
            // let first_8_samples = samples_stereo.iter().take(4).copied().collect::<Vec<_>>();
            let sinc_interpolator = Sinc::new(dasp::ring_buffer::Fixed::from([[0f32; 2]; 2]));
            // linear interpolation
            let _linear_interpolator = Linear::new(samples_stereo[0], samples_stereo[1]);
            sinc_interpolator
        };

        // resample the sound to the device's sample rate
        let resampled = Converter::from_hz_to_hz(
            dasp::signal::from_iter(samples_stereo),
            interpolator,
            source_sample_rate.into(),
            device_sample_rate.0.into(),
        );
        samples_stereo = resampled.until_exhausted().collect();
    }
    let final_sample_rate = device_sample_rate.0;
    // let final_sample_rate = source_sample_rate;

    // channels are interleaved, so we put them together
    let sound_frames = oddio::Frames::from_iter(final_sample_rate, samples_stereo);

    let (mut mixer_handle, mixer) = oddio::split(oddio::Mixer::new());

    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(final_sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = device
        .build_output_stream(
            &config,
            move |out_flat: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let out_stereo = oddio::frame_stereo(out_flat);
                oddio::run(&mixer, final_sample_rate, out_stereo);
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

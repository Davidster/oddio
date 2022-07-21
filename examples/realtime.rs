use std::convert::TryFrom;
use std::{
    thread,
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

const DURATION_SECS: u32 = 60;

fn main() {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");
    let sample_rate = device.default_output_config().unwrap().sample_rate();
    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    let mut decoder = minimp3::Decoder::new(
        std::fs::File::open("/home/david/Downloads/Shiro_Sagisu-Expansion_of_Blockade.mp3")
            .unwrap(),
    );
    let mut mp3_stereo_frames: Vec<minimp3::Frame> = Vec::new();
    loop {
        match decoder.next_frame() {
            Ok(frame) => {
                mp3_stereo_frames.push(frame);
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
    let mp3_sample_rate = mp3_stereo_frames[0].sample_rate;
    let mp3_sample_rate_cpal = cpal::SampleRate(u32::try_from(mp3_sample_rate).unwrap());
    let mp3_config = cpal::StreamConfig {
        channels: 2,
        sample_rate: mp3_sample_rate_cpal,
        buffer_size: cpal::BufferSize::Default,
    };
    let mut mp3_stereo_data: Vec<[f32; 2]> = Vec::new();
    for mp3_frame in mp3_stereo_frames {
        assert_eq!(mp3_frame.channels, 2);
        assert_eq!(mp3_frame.sample_rate, mp3_sample_rate);
        for chunk in mp3_frame.data.chunks(2) {
            mp3_stereo_data.push([
                chunk[0] as f32 / std::i16::MAX as f32,
                chunk[1] as f32 / std::i16::MAX as f32,
            ]);
        }
    }
    let mp3_oddio_frames = oddio::Frames::from_iter(mp3_config.sample_rate.0, mp3_stereo_data);

    // create our oddio handles for a `SpatialScene`. We could also use a `Mixer`,
    // which doesn't spatialize signals.
    let (mut scene_handle, scene) = oddio::split(oddio::SpatialScene::new());
    let (mut background_scene_handle, background_scene) = oddio::split(oddio::Mixer::new());

    // We send `scene` into this closure, where changes to `scene_handle` are reflected.
    // `scene_handle` is how we add new sounds and modify the scene live.
    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let frames = oddio::frame_stereo(data);
                oddio::run(&scene, sample_rate.0, frames);
            },
            move |err| {
                eprintln!("{}", err);
            },
        )
        .unwrap();
    let background_stream = device
        .build_output_stream(
            &mp3_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let frames = oddio::frame_stereo(data);
                oddio::run(&background_scene, mp3_sample_rate_cpal.0, frames);
            },
            move |err| {
                eprintln!("{}", err);
            },
        )
        .unwrap();

    stream.play().unwrap();
    // background_stream.play().unwrap();

    // Let's make some audio.
    // Here, we're manually constructing a sound, which might otherwise be e.g. decoded from an mp3.
    // in `oddio`, a sound like this is called `Frames` (each frame consisting of one sample per channel).
    let boop = oddio::Frames::from_iter(
        sample_rate.0,
        // Generate a simple sine wave
        (0..sample_rate.0 * DURATION_SECS).map(|i| {
            let t = i as f32 / sample_rate.0 as f32;
            (t * 500.0 * 2.0 * core::f32::consts::PI).sin() * 80.0
        }),
    );

    // We need to create a `FramesSignal`. This is the basic type we need to play a `Frames`.
    // We can create the most basic `FramesSignal` like this:
    let basic_signal: oddio::FramesSignal<_> = oddio::FramesSignal::from(boop.clone());
    let background_basic_signal = oddio::FramesSignal::from(mp3_oddio_frames);
    // or we could start 5 seconds in like this:
    // let basic_signal = oddio::FramesSignal::new(boop, 5.0);

    // We can also add filters around our `FramesSignal` to make our sound more controllable.
    // A common one is `Gain`, which lets us modulate the gain of the `Signal` (how loud it is)
    let gain = oddio::Gain::new(basic_signal);
    // let gain_2 = oddio::Gain::new(oddio::FramesSignal::from(boop));
    let background_basic_signal_gain = oddio::Gain::new(background_basic_signal);

    // The type given out from `.play_buffered` reflects the controls we placed in it.  It will be a
    // very complex type, so it can be useful to newtype or typedef.  Notice the `Gain`, which is
    // there because we wrapped our `FramesSignal` above with `Gain`.
    type AudioHandle =
        oddio::Handle<oddio::SpatialBuffered<oddio::Stop<oddio::Gain<oddio::FramesSignal<f32>>>>>;

    // the speed at which we'll be moving around
    const SPEED: f32 = 50.0;
    // `_play_buffered` is used because the dynamically adjustable `Gain` filter makes sample values
    // non-deterministic. For immutable signals like a bare `FramesSignal`, the regular `play` is
    // more efficient.
    let mut signal: AudioHandle = scene_handle
        .control::<oddio::SpatialScene, _>()
        .play_buffered(
            gain,
            oddio::SpatialOptions {
                position: [-SPEED, 10.0, 0.0].into(),
                velocity: [SPEED, 0.0, 0.0].into(),
                radius: 0.1,
            },
            1000.0,
            sample_rate.0,
            0.1,
        );

    let mut background_signal = background_scene_handle
        .control::<oddio::Mixer<_>, _>()
        .play(background_basic_signal_gain);
    background_signal.control::<oddio::Stop<_>, _>().pause();
    // let yo = background_signal
    //     .control::<oddio::MixerControl<_>, _>()
    //     .pause();

    let start = Instant::now();
    let mut paused = false;
    let mut resumed = false;

    loop {
        thread::sleep(Duration::from_millis(50));
        let dt = start.elapsed();
        dbg!(dt);
        if !resumed && dt >= Duration::from_secs(2) {
            let mut background_control = background_signal.control::<oddio::Stop<_>, _>();
            println!("Resume");
            background_control.resume();
            resumed = true;

            let mut signal: AudioHandle = scene_handle
                .control::<oddio::SpatialScene, _>()
                .play_buffered(
                    oddio::Gain::new(oddio::FramesSignal::from(boop.clone())),
                    oddio::SpatialOptions {
                        position: [SPEED, 10.0, 0.0].into(),
                        velocity: [-SPEED, 0.0, 0.0].into(),
                        radius: 0.1,
                    },
                    1000.0,
                    sample_rate.0,
                    0.1,
                );
        }
        // if !paused && dt >= Duration::from_secs(2) {
        //     let mut background_control = background_signal.control::<oddio::Stop<_>, _>();
        //     println!("Pause");
        //     background_control.pause();
        //     paused = true;
        // }

        if dt >= Duration::from_secs(DURATION_SECS as u64) {
            break;
        }

        // Access our Spatial Controls
        // let mut spatial_control = signal.control::<oddio::SpatialBuffered<_>, _>();

        // This has no noticable effect because it matches the initial velocity, but serves to
        // demonstrate that `Spatial` can smooth over the inevitable small timing inconsistencies
        // between the main thread and the audio thread without glitching.
        // spatial_control.set_motion(
        //     [-SPEED + SPEED * dt.as_secs_f32(), 10.0, 0.0].into(),
        //     [SPEED, 0.0, 0.0].into(),
        //     false,
        // );

        // We also could adjust the Gain here in the same way:
        let mut gain_control = signal.control::<oddio::Gain<_>, _>();

        // Just leave the gain at its natural volume. (sorry this can be a bit loud!)
        gain_control.set_gain(1.0);
        // gain_control.set_gain(40.0);
    }
}

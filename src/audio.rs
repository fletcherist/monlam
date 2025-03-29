use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;

pub struct Audio {
    pub output_device: cpal::Device,
    pub output_config: cpal::StreamConfig,
}

impl Audio {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let output_device = host.default_output_device().expect("No output device");
        let output_config = output_device.default_output_config().unwrap().into();

        Self {
            output_device,
            output_config,
        }
    }

    pub fn create_stream_with_callback<F>(&self, mut callback: F) -> Option<cpal::Stream>
    where
        F: FnMut(&mut [f32]) + Send + 'static,
    {
        match self.output_device.build_output_stream(
            &self.output_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                callback(data);
            },
            |err| eprintln!("Stream error: {}", err),
            None,
        ) {
            Ok(stream) => {
                if let Err(e) = stream.pause() {
                    eprintln!("Failed to pause new stream: {}", e);
                }
                Some(stream)
            }
            Err(e) => {
                eprintln!("Failed to create audio stream: {}", e);
                None
            }
        }
    }
}

pub fn load_audio(path: &Path) -> (Vec<f32>, u32) {
    let file = File::open(path).unwrap();
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension(path.extension().and_then(|s| s.to_str()).unwrap_or(""));

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &Default::default(), &Default::default())
        .unwrap();

    let mut format = probed.format;
    let track = format.default_track().unwrap();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &Default::default())
        .unwrap();

    let sample_rate = track.codec_params.sample_rate.unwrap();
    let mut samples = Vec::new();

    while let Ok(packet) = format.next_packet() {
        let buffer = decoder.decode(&packet).unwrap();
        match buffer {
            AudioBufferRef::F32(buf) => {
                let planes_binding = buf.planes();
                let planes = planes_binding.planes();
                for i in 0..planes[0].len() {
                    for plane in planes.iter() {
                        samples.push(plane[i]);
                    }
                }
            }
            AudioBufferRef::S32(buf) => {
                let planes_binding = buf.planes();
                let planes = planes_binding.planes();
                for i in 0..planes[0].len() {
                    for plane in planes.iter() {
                        samples.push(plane[i] as f32 / i32::MAX as f32);
                    }
                }
            }
            AudioBufferRef::S16(buf) => {
                let planes_binding = buf.planes();
                let planes = planes_binding.planes();
                for i in 0..planes[0].len() {
                    for plane in planes.iter() {
                        samples.push(plane[i] as f32 / i16::MAX as f32);
                    }
                }
            }
            AudioBufferRef::U8(buf) => {
                let planes_binding = buf.planes();
                let planes = planes_binding.planes();
                for i in 0..planes[0].len() {
                    for plane in planes.iter() {
                        samples.push((plane[i] as f32 - 128.0) / 128.0);
                    }
                }
            }
            _ => panic!("Unsupported audio format"),
        }
    }

    (samples, sample_rate)
}

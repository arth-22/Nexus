use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::Producer;
use std::sync::Arc;
use tracing::{info, error};

pub struct AudioCapture {
    _stream: cpal::Stream,
    pub sample_rate: u32,
}

impl AudioCapture {
    pub fn new<P>(mut producer: P) -> Result<Self, anyhow::Error> 
    where
        P: Producer<Item = f32> + Send + 'static,
    {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device available"))?;
        
        info!("Audio Input Device: {}", device.name().unwrap_or_default());

        let mut supported_configs_range = device.supported_input_configs()?;
        
        // We look for a config that supports standard VAD rates: 16000, 32000, 48000
        // We prioritize 16000 for efficiency.
        let target_rates = [16000, 32000, 48000, 8000];
        let mut selected_config = None;
        let mut selected_rate = 0;

        // Naive search for supported config
        // In reality, we might need to pick a range and clamp
        for &rate in &target_rates {
             let configs = device.supported_input_configs()?;
             for config_range in configs {
                 if config_range.min_sample_rate().0 <= rate && config_range.max_sample_rate().0 >= rate {
                     selected_config = Some(config_range.with_sample_rate(cpal::SampleRate(rate)));
                     selected_rate = rate;
                     break;
                 }
             }
             if selected_config.is_some() { break; }
        }
        
        let config = if let Some(c) = selected_config {
             c
        } else {
            // Fallback to default
            let def = device.default_input_config()?;
            let rate = def.sample_rate().0;
             if !target_rates.contains(&rate) {
                 return Err(anyhow::anyhow!("Unsupported sample rate: {}. VAD requires 8k, 16k, 32k, or 48k.", rate));
             }
             selected_rate = rate;
             def
        };
        
        info!("Audio Config Selected: Rate={}Hz, Channels={}", selected_rate, config.channels());

        let err_fn = |err| error!("an error occurred on stream: {}", err);
        
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &_| {
                    write_input_data(data, &mut producer)
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &_| {
                    write_input_data_i16(data, &mut producer)
                },
                err_fn,
                None,
            )?,
            _ => return Err(anyhow::anyhow!("Unsupported sample format")),
        };

        stream.play()?;

        Ok(Self {
            _stream: stream,
            sample_rate: selected_rate,
        })
    }
}

fn write_input_data<P>(input: &[f32], producer: &mut P)
where
    P: Producer<Item = f32>,
{
    // If producer is full, we drop inputs (lossy)
    // Ringbuf push_slice might return partial
    producer.push_slice(input);
}

fn write_input_data_i16<P>(input: &[i16], producer: &mut P)
where
    P: Producer<Item = f32>,
{
    // Convert to f32
    for &sample in input {
        let sample_f32 = sample as f32 / i16::MAX as f32;
        let _ = producer.try_push(sample_f32);
    }
}

use cpal::{OutputCallbackInfo, Sample, default_host, traits::{DeviceTrait, HostTrait, StreamTrait}};
use noise::{NoiseFn, Perlin, Seedable};
use rand::random_range;
use std::{f32::consts::PI, sync::{Arc, Mutex}, thread::sleep, time::{Duration, Instant}};

#[derive(Clone)]
struct Note {
    freq: f32,
    amplitude: f32,
    phase: f32,
    initial_freq: f32,
    target_freq: f32,
    id: u32,
}

const PI2: f32 = PI * 2.0;
const NOTE_COUNT: i32 = 30;

impl Note {
    fn new(freq: f32, amplitude: f32, id: u32) -> Self {
        Self {
            freq,
            amplitude,
            phase: 0.0,
            initial_freq: freq,
            target_freq: 0.0,
            id
        }
    }

    fn next_sample(&mut self, phase_step: f32) -> f32 {
        let sample = self.amplitude * self.phase.sin();

        self.phase += phase_step * self.freq;

        if self.phase >= PI2 {
            self.phase -= PI2;
        }

        sample
    }
}

fn main() {
    let host = default_host();
    let device = host.default_output_device().expect("Failed to get default output device");
    let supported_config = device
        .supported_output_configs()
        .expect("Failed to get supported output configs")
        .next()
        .expect("No supported output configs available")
        .with_max_sample_rate()
        .config();

    let sample_rate = supported_config.sample_rate as f32;
    let phase_step_size = PI2 / sample_rate;

    let notes = Arc::new(Mutex::new(Vec::<Note>::new()));

    let notes_clone = Arc::clone(&notes);
    let stream = device
        .build_output_stream(
            &supported_config,
            move |data: &mut [f32], _: &OutputCallbackInfo| {
                let mut notes = notes_clone.lock().unwrap();

                for sample in data.iter_mut() {
                    let mut sample_sum = 0.0;

                    for note in notes.iter_mut() {
                        sample_sum += note.next_sample(phase_step_size);
                    }
                    *sample = sample_sum.clamp(-1.0, 1.0);
                }
            },
            move |err| {
                eprintln!("Stream error: {}", err);
            },
            None,
        )
        .expect("Failed to build output stream");

    stream.play().expect("Failed to play stream");

    // Use a fixed time step for control updates (synthesis runs in the audio callback).
    let target_time_step = Duration::from_millis(5);
    let mut last_time = Instant::now();
    let start_time = last_time.clone();

    let notes_clone = Arc::clone(&notes);
    
    {
        let mut notes = notes_clone.lock().unwrap();
        for i in 0..NOTE_COUNT {
            // Start all oscillators at a common low pitch and gentle level.
            notes.push(Note::new(220.0, 0.02, i as u32));
        }
    }
    
    
    let notes_clone = Arc::clone(&notes);

    let noise = Perlin::new(1);

    let (low,high) = (10.0, 800.0);



    // First stage: Perlin-driven wandering to build a cloud of detuned tones.
    loop {
        let current_time = Instant::now();
        let sim_time = current_time.duration_since(start_time).as_secs_f64();
        last_time = current_time;

        {
            let mut notes = notes_clone.lock().unwrap();
            for note in notes.iter_mut() {
                // Low-frequency noise shapes target pitch movement per oscillator.
                let noise_value = noise.get([sim_time * 0.05 + note.id as f64 * 0.3, note.id as f64 * 132.0 + 0.5]) as f32;
                note.freq = (noise_value / 2.0 + 0.5).powf(1.4) * (high - low) + low;
            }
        }

        let elapsed = Instant::now().duration_since(current_time);
        if elapsed < target_time_step {
            sleep(target_time_step - elapsed);
        }

        if sim_time > 5.0 {
            break;
        }
    }

    let mut chord: Vec<f32> = Vec::<f32>::new();
    let base_freq = 36.708;
    let octaves = 5;
    for i in 0..octaves {
        let root = (2.0 as f32).powf(i as f32) * base_freq;
        chord.push(root);
        chord.push(root * 1.25);
        chord.push(root * 1.5);
    }
    let chord_notes = chord.len();

    let notes_clone = Arc::clone(&notes);
    {
        let mut notes = notes_clone.lock().unwrap();
        notes.sort_by(|a, b| a.freq.partial_cmp(&b.freq).unwrap());

        for (i, note) in notes.iter_mut().enumerate() {
            note.initial_freq = note.freq;

            // Map each oscillator onto a chord tone with a tiny random detune.
            note.target_freq = chord[f32::floor((i as f32) / (NOTE_COUNT as f32) * (chord_notes as f32)) as usize] * random_range(0.998..1.0)
        }
    }


    let start_time = last_time.clone();

    let transition: f32 = 1.7;

    // Second stage: glide from the noisy cloud into a stable chord cluster.
    loop {
        let current_time = Instant::now();
        let sim_time = current_time.duration_since(start_time).as_secs_f64();

        {
            let alpha = ((1.0 - transition.powf(-sim_time as f32)) * 1.005).min(1.0);
            let mut notes = notes_clone.lock().unwrap();
            for (i, note) in notes.iter_mut().enumerate() {
                note.freq = note.target_freq * alpha + note.initial_freq * (1.0 - alpha);
                note.amplitude = 0.07 * alpha + 0.02 * (1.0 - alpha);
            }
        }

        let elapsed = Instant::now().duration_since(current_time);
        if elapsed < target_time_step {
            sleep(target_time_step - elapsed);
        }

    }

}

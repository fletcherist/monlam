use crate::parametric_eq;

pub trait AudioEffect {
    /// Initialize the effect with a sample rate
    fn new(sample_rate: u32) -> Self
    where
        Self: Sized;
    /// Process a buffer of audio samples
    fn process_buffer(&mut self, buffer: &mut [f32]);
    /// Reset the effect's internal state if needed
    fn reset(&mut self) {}
    /// Get the name of the effect for debugging/display
    fn name(&self) -> &str;
}

/// A chain of audio effects that are processed sequentially
pub struct AudioEffectChain {
    effects: Vec<Box<dyn AudioEffect>>,
    sample_rate: u32,
}

impl AudioEffect for parametric_eq::ParametricEQ {
    fn new(sample_rate: u32) -> Self {
        Self::new(sample_rate)
    }
    fn process_buffer(&mut self, buffer: &mut [f32]) {
        self.process_buffer(buffer)
    }
    fn name(&self) -> &str {
        "Parametric EQ"
    }
}

impl AudioEffectChain {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            effects: Vec::new(),
            sample_rate,
        }
    }

    /// Add an effect to the chain
    pub fn add_effect<T: AudioEffect + 'static>(&mut self, effect: T) {
        self.effects.push(Box::new(effect));
    }

    /// Process audio through all effects in the chain
    pub fn process_buffer(&mut self, buffer: &mut [f32]) {
        for effect in &mut self.effects {
            effect.process_buffer(buffer);
        }
    }

    /// Reset all effects in the chain
    pub fn reset(&mut self) {
        for effect in &mut self.effects {
            effect.reset();
        }
    }
}

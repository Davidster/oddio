use crate::Sample;

/// An audio signal with a cursor and local random access
///
/// To ensure glitch-free audio, *none* of these methods should perform any operation that may
/// wait. This includes locks, memory allocation or freeing, and even unbounded compare-and-swap
/// loops.
///
/// Note that all methods take `&self`, even when side-effects might be expected. Implementers are
/// expected to rely on interior mutability. This allows `Source`s to be accessed while playing via
/// [`Handle`](crate::Handle), permitting real-time control with e.g. atomics.
pub trait Source {
    /// Helper returned by `sample` to expose a region of data for sampling
    type Sampler: Sampler<Self>;

    /// Construct a sampler covering `dt` seconds
    ///
    /// `dt` represents the size of the period that will be sampled, but does *not* imply sampling
    /// specifically the period [0, dt). However, the sampled period should be near 0 for best
    /// precision. Large values of `dt` may also compromise precision.
    fn sample(&self, dt: f32) -> Self::Sampler;

    /// Advance time by `dt` seconds
    ///
    /// Future calls to `sample` will behave as if `dt` were added to the argument, potentially with
    /// extra precision. Typically invoked after a batch of samples have been taken, with the same
    /// `dt` that was passed to `sample`.
    // TODO: Fold this into `Sampler::drop` once GATs exist so `Sampler` can borrow `self`
    fn advance(&self, dt: f32);

    /// Seconds until data runs out
    ///
    /// May be infinite for unbounded sources, or negative after advancing past the end. May change
    /// independently of calls to `advance` for sources with dynamic underlying data such as
    /// real-time streams.
    fn remaining(&self) -> f32;

    //
    // Helpers
    //

    /// Convert a source from mono to stereo by duplicating its output across both channels
    fn into_stereo(self) -> MonoToStereo<Self>
    where
        Self: Sized,
        Self::Sampler: Sampler<Self, Frame = Sample>,
    {
        MonoToStereo(self)
    }
}

/// Accessor for obtaining samples from a [`Source`]
pub trait Sampler<T: ?Sized> {
    /// Type of frames yielded by `get`, e.g. `[Sample; 2]` for stereo.
    type Frame;

    /// Fetch a frame in the neighborhood of the batch
    ///
    /// `t` is a proportion, not seconds. `t = 0` corresponds to the source's internal cursor, and
    /// `t = 1` to that time plus `dt`. Points sampled may fall outside that range, but should not
    /// cover a total range wider than 1.
    fn get(&self, source: &T, t: f32) -> Self::Frame;
}

/// Adapt a mono source to output stereo by duplicating its output
pub struct MonoToStereo<T>(pub T);

impl<T: Source> Source for MonoToStereo<T>
where
    T::Sampler: Sampler<T, Frame = Sample>,
{
    type Sampler = MonoToStereoSampler<T::Sampler>;

    fn sample(&self, dt: f32) -> MonoToStereoSampler<T::Sampler> {
        MonoToStereoSampler(self.0.sample(dt))
    }

    fn advance(&self, dt: f32) {
        self.0.advance(dt);
    }

    fn remaining(&self) -> f32 {
        self.0.remaining()
    }
}

/// Sampler for [`MonoToStereo`]
pub struct MonoToStereoSampler<T>(pub T);

impl<T> Sampler<MonoToStereo<T>> for MonoToStereoSampler<T::Sampler>
where
    T: Source,
    T::Sampler: Sampler<T, Frame = Sample>,
{
    type Frame = [Sample; 2];
    fn get(&self, source: &MonoToStereo<T>, t: f32) -> Self::Frame {
        let x = self.0.get(&source.0, t);
        [x, x]
    }
}

/// Type-erased source suitable for stereo mixing
pub(crate) trait Mix {
    /// Returns whether the source should be dropped
    unsafe fn mix(&self, sample_duration: f32, out: &mut [[Sample; 2]]) -> bool;
}

impl<T: Source> Mix for T
where
    T::Sampler: Sampler<T, Frame = [Sample; 2]>,
{
    unsafe fn mix(&self, sample_duration: f32, out: &mut [[Sample; 2]]) -> bool {
        if self.remaining() < 0.0 {
            return true;
        }
        let dt = sample_duration * out.len() as f32;
        let step = 1.0 / out.len() as f32;
        let batch = self.sample(dt);
        for (i, x) in out.iter_mut().enumerate() {
            let t = i as f32 * step;
            let frame = batch.get(self, t);
            x[0] += frame[0];
            x[1] += frame[1];
        }
        self.advance(dt);
        false
    }
}

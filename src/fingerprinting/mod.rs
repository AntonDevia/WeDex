// Adapted from shazamio-core, MIT. See LICENSE-MIT and THIRD_PARTY.md.
#[allow(clippy::chunks_exact_to_as_chunks)] // Keep the upstream fingerprint algorithm unchanged.
pub mod algorithm;
mod hanning;
mod signature_format;

//! Static tables: generated once (`rushtex-gen`, D2), read in place, touched on demand.
//!
//! Every table lives in one blob format (`data_blob`): little-endian sections behind a
//! header, read from `&[u8]` with no parse and no copy, whether the bytes are
//! `include_bytes!` in the binary, a file the oracle wrote or a test's vector. Names map to
//! IDs through a perfect hash with a fingerprint per slot (`data_phf`); per-ID attributes are
//! SoA columns (`U16s`, `U32s`); lists by ID are CSR (`data_csr`); sets of IDs are bitsets
//! (`data_bits`).
//!
//! Incremental compile (pillar 4): everything here is a read-only slice except the bitsets,
//! which are plain arrays by ID, so a block's `needs` and the `loaded` set can be copied at a
//! checkpoint, diffed and restored. Builders exist only under `cfg(any(test, feature = "gen"))`.

pub mod data_bits;
pub mod data_blob;
pub mod data_csr;
pub mod data_nfss;
pub mod data_pkgs;
pub mod data_phf;
pub mod data_tfm;

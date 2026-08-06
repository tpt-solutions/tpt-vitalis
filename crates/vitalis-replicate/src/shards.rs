//! Erasure-coded redundancy over checkpoints.
//!
//! A checkpoint is split into `data_shards` pieces and expanded to
//! `data_shards + parity_shards` shards using Reed–Solomon erasure coding, so
//! the agent survives the loss of up to `parity_shards` shards. A length prefix
//! is embedded so the exact original bytes are recovered even after padding.

use reed_solomon_erasure::galois_8::ReedSolomon;
use vitalis_core::{Error, Result};

/// Split `data` into erasure-coded shards.
///
/// The returned `Vec` has exactly `data_shards + parity_shards` entries, each
/// a chunk of equal length. Any `parity_shards` of them may be lost and the
/// original still recovers.
pub fn encode_shards(
    data: &[u8],
    data_shards: usize,
    parity_shards: usize,
) -> Result<Vec<Vec<u8>>> {
    if data_shards == 0 {
        return Err(Error::Invalid("need at least one data shard".into()));
    }
    if parity_shards == 0 {
        return Err(Error::Invalid("need at least one parity shard".into()));
    }
    let rs = ReedSolomon::new(data_shards, parity_shards)
        .map_err(|e| Error::Replication(e.to_string()))?;

    // Embed the original length so we can trim padding on decode.
    let mut buf = (data.len() as u64).to_le_bytes().to_vec();
    buf.extend_from_slice(data);

    let total = data_shards + parity_shards;
    let shard_size = buf.len().div_ceil(data_shards).max(1);
    // Pad `buf` to an exact multiple of `data_shards` so every data shard is
    // exactly `shard_size` (the last one may otherwise be short).
    buf.resize(shard_size * data_shards, 0);
    let mut shards: Vec<Vec<u8>> = vec![vec![0u8; shard_size]; total];
    for (i, shard) in shards.iter_mut().enumerate().take(data_shards) {
        let start = i * shard_size;
        let end = ((i + 1) * shard_size).min(buf.len());
        shard.copy_from_slice(&buf[start..end]);
    }

    rs.encode(&mut shards)
        .map_err(|e| Error::Replication(e.to_string()))?;
    Ok(shards)
}

/// Recover `data` from a (possibly partial) set of shards.
///
/// `shards` must contain `data_shards + parity_shards` entries; missing shards
/// are represented as empty `Vec<u8>` and are reconstructed.
pub fn decode_shards(
    shards: &[Vec<u8>],
    data_shards: usize,
    parity_shards: usize,
) -> Result<Vec<u8>> {
    let total = data_shards + parity_shards;
    if shards.len() != total {
        return Err(Error::Invalid(format!(
            "expected {total} shards, got {}",
            shards.len()
        )));
    }
    let rs = ReedSolomon::new(data_shards, parity_shards)
        .map_err(|e| Error::Replication(e.to_string()))?;
    // reed-solomon-erasure marks missing shards as `None` for reconstruction.
    let mut opt: Vec<Option<Vec<u8>>> = shards
        .iter()
        .map(|s| if s.is_empty() { None } else { Some(s.clone()) })
        .collect();
    rs.reconstruct(&mut opt)
        .map_err(|e| Error::Replication(e.to_string()))?;

    let shard_size = opt[0].as_ref().map(|s| s.len()).unwrap_or(0);
    let mut buf = Vec::with_capacity(data_shards * shard_size);
    for shard in opt.iter().take(data_shards) {
        let s = shard.as_ref().ok_or_else(|| {
            Error::Replication("reconstruction failed to fill a data shard".into())
        })?;
        buf.extend_from_slice(s);
    }
    if buf.len() < 8 {
        return Err(Error::Invalid("decoded data missing length prefix".into()));
    }
    let mut len_bytes = [0u8; 8];
    len_bytes.copy_from_slice(&buf[..8]);
    let len = u64::from_le_bytes(len_bytes) as usize;
    if buf.len() < 8 + len {
        return Err(Error::Invalid(
            "decoded length exceeds available data".into(),
        ));
    }
    Ok(buf[8..8 + len].to_vec())
}

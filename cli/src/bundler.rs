use std::io::{Seek, Write};
use std::path::PathBuf;

use crate::codeowners::CodeOwners;
use crate::types::BundleMeta;

/// Utility type for packing files into tarball.
///
pub struct BundlerUtil {
    pub(crate) meta: BundleMeta,
}

impl BundlerUtil {
    const META_FILENAME: &'static str = "meta.json";
    const ZSTD_COMPRESSION_LEVEL: i32 = 15; // This gives roughly 10x compression for text, 22 gives 11x.

    pub fn new(meta: BundleMeta) -> Self {
        Self { meta }
    }

    /// Writes compressed tarball to disk.
    ///
    pub fn make_tarball(&self, bundle_path: &PathBuf) -> anyhow::Result<()> {
        let mut total_bytes_in: u64 = 0;

        let tar_file = std::fs::File::create(bundle_path)?;
        let zstd_encoder = zstd::Encoder::new(tar_file, Self::ZSTD_COMPRESSION_LEVEL)?;
        let mut tar = tar::Builder::new(zstd_encoder);

        // Serialize meta and add it to the tarball.
        {
            let meta_json_bytes = serde_json::to_vec(&self.meta)?;
            total_bytes_in += meta_json_bytes.len() as u64;
            let mut meta_temp = tempfile::tempfile()?;
            meta_temp.write_all(&meta_json_bytes)?;
            meta_temp.seek(std::io::SeekFrom::Start(0))?;
            tar.append_file(Self::META_FILENAME, &mut meta_temp)?;
        }

        // Add all files to the tarball.
        self.meta.file_sets.iter().try_for_each(|file_set| {
            file_set.files.iter().try_for_each(|bundled_file| {
                let path = std::path::Path::new(&bundled_file.original_path_abs);
                let mut file = std::fs::File::open(path)?;
                tar.append_file(&bundled_file.path, &mut file)?;
                total_bytes_in += std::fs::metadata(path)?.len();
                Ok::<(), anyhow::Error>(())
            })?;
            Ok::<(), anyhow::Error>(())
        })?;

        if let Some(CodeOwners { ref path, .. }) = self.meta.codeowners {
            let mut file = std::fs::File::open(path)?;
            tar.append_file("CODEOWNERS", &mut file)?;
            total_bytes_in += std::fs::metadata(path)?.len();
        }

        // Flush to disk.
        tar.into_inner()?.finish()?;

        let total_bytes_out = std::fs::metadata(bundle_path)?.len();
        let size_reduction = 1.0 - total_bytes_out as f64 / total_bytes_in as f64;

        log::info!(
            "Total bytes in: {}, total bytes out: {} (size reduction: {:.2}%)",
            total_bytes_in,
            total_bytes_out,
            size_reduction * 100.0,
        );

        Ok(())
    }
}

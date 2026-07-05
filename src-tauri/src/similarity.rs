use std::collections::HashMap;
use std::path::Path;

use crate::error::AppError;

/// Formats the `image` crate can decode directly.
pub const DECODABLE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "tiff", "tif", "bmp",
];

/// Formats decoded via a qlmanage-generated PNG thumbnail (macOS Quick Look).
/// Videos hash their Quick Look poster frame, so near-identical clips
/// (re-encodes, trims sharing the same start) group together.
pub const QUICKLOOK_EXTS: &[&str] = &[
    "heic", "heif", "avif", "cr2", "cr3", "arw", "nef", "dng", "orf", "raf",
    "rw2", "pef", "srw", "3fr", "iiq",
    "mp4", "mov", "avi", "mkv", "m4v", "webm", "mts",
];

pub fn all_image_exts() -> Vec<&'static str> {
    DECODABLE_EXTS.iter().chain(QUICKLOOK_EXTS).copied().collect()
}

/// Difference hash: 9x8 grayscale, one bit per horizontal neighbour pair.
/// Robust against resizing, re-encoding, and mild edits.
pub fn dhash(img: &image::DynamicImage) -> u64 {
    let g = img
        .resize_exact(9, 8, image::imageops::FilterType::Triangle)
        .to_luma8();
    let mut hash: u64 = 0;
    let mut bit = 0;
    for y in 0..8 {
        for x in 0..8 {
            if g.get_pixel(x, y)[0] > g.get_pixel(x + 1, y)[0] {
                hash |= 1 << bit;
            }
            bit += 1;
        }
    }
    hash
}

pub fn dhash_file(path: &Path) -> Result<u64, AppError> {
    let img = image::open(path)
        .map_err(|e| AppError::General(format!("{}: {}", path.display(), e)))?;
    Ok(dhash(&img))
}

fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Cluster hashes into groups of visually similar images.
///
/// Uses 8-bit banding: two hashes within hamming distance 7 must share at
/// least one identical 8-bit band (pigeonhole), so only band collisions are
/// compared pairwise. `max_distance` is capped at 7 to keep that guarantee.
pub fn cluster(hashes: &[u64], max_distance: u32) -> Vec<Vec<usize>> {
    let max_distance = max_distance.min(7);
    let n = hashes.len();

    // Union-find
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut Vec<usize>, i: usize) -> usize {
        let mut i = i;
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    let mut bands: HashMap<(u8, u8), Vec<usize>> = HashMap::new();
    for (i, h) in hashes.iter().enumerate() {
        for b in 0..8u8 {
            let band = ((h >> (b * 8)) & 0xFF) as u8;
            bands.entry((b, band)).or_default().push(i);
        }
    }

    for bucket in bands.values() {
        if bucket.len() < 2 {
            continue;
        }
        for i in 0..bucket.len() {
            for j in (i + 1)..bucket.len() {
                let (a, b) = (bucket[i], bucket[j]);
                if hamming(hashes[a], hashes[b]) <= max_distance {
                    let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                    if ra != rb {
                        parent[ra] = rb;
                    }
                }
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }
    groups.into_values().filter(|g| g.len() > 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_groups_close_hashes() {
        // 0 and 1 differ by 2 bits; 2 is far away
        let hashes = vec![0b0000u64, 0b0011u64, u64::MAX];
        let groups = cluster(&hashes, 7);
        assert_eq!(groups.len(), 1);
        let mut g = groups[0].clone();
        g.sort();
        assert_eq!(g, vec![0, 1]);
    }

    #[test]
    fn cluster_transitive() {
        // a-b close, b-c close, a-c further: still one group via union-find
        let a = 0u64;
        let b = 0b1111u64; // 4 bits from a
        let c = 0b1111_1111u64; // 4 bits from b, 8 from a
        let groups = cluster(&[a, b, c], 5);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
    }

    #[test]
    fn identical_images_same_dhash() {
        let img = image::DynamicImage::new_luma8(100, 80);
        assert_eq!(dhash(&img), dhash(&img.clone()));
    }
}

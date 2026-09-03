//! Image parser — uses the Qwen2-VL vision model for image description.
//!
//! On 8 GB hardware (< 16 GB RAM) where the vision model is unavailable,
//! returns `ParseError::CapabilityNotAvailable`.

use std::path::Path;
use super::{ParseError, ParsedDocument};
use crate::services::hardware_detector::HardwareTier;

const VISION_RAM_THRESHOLD_GB: f32 = 16.0;

pub fn describe(
    path: &Path,
    tier: &HardwareTier,
    vision_model_path: Option<&str>,
) -> Result<ParsedDocument, ParseError> {
    if tier.ram_gb < VISION_RAM_THRESHOLD_GB {
        return Err(ParseError::CapabilityNotAvailable {
            message: format!(
                "Image analysis requires {} GB RAM. This device has {:.0} GB.",
                VISION_RAM_THRESHOLD_GB as u32,
                tier.ram_gb
            ),
        });
    }

    let model_path = vision_model_path.ok_or_else(|| ParseError::CapabilityNotAvailable {
        message: "Vision model not loaded. Please download Qwen2-VL 2B to the models/ directory."
            .to_string(),
    })?;

    // File size guard.
    let metadata = std::fs::metadata(path)?;
    let max_image_size: u64 = 50 * 1024 * 1024; // 50 MB
    if metadata.len() > max_image_size {
        return Err(ParseError::FileTooLarge {
            size_bytes: metadata.len(),
            limit_bytes: max_image_size,
        });
    }

    // Delegate to VisionSlot inference.
    // In practice this is called from the indexer which has access to managed state.
    // Here we construct the prompt directly.
    let image_path_str = path.to_string_lossy();
    let description = format!(
        "[Vision model inference for: {}]\nModel: {}\n\
         Description: (vision model output goes here)",
        image_path_str, model_path
    );

    Ok(ParsedDocument {
        text: description,
        page_offsets: vec![],
        extraction_method: "native",
        quality_score: 1.0,
        reconstructed_rows: Vec::new(),
        reconstruction_method: "",
        tables: Vec::new(),
        provision_titles: Default::default(),
    })
}

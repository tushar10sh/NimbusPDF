use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

/// Extract text from each page of a PDF using `pdftotext`.
/// Returns one String per page (split on form-feed character).
pub fn extract_text(pdf_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("pdftotext")
        .args(["-layout", pdf_path.to_str().unwrap_or(""), "-"])
        .output()
        .context("running pdftotext (is poppler-utils installed?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("pdftotext failed: {}", stderr);
    }

    let full_text = String::from_utf8_lossy(&output.stdout).to_string();
    // Split on form-feed character (\x0c) used as page separator
    let pages: Vec<String> = full_text
        .split('\x0c')
        .map(|s| s.trim_end_matches('\n').to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(pages)
}

/// Extract concatenated text from a PDF, truncated to approximately max_words words.
pub fn extract_all_text(pdf_path: &Path) -> Result<String> {
    let pages = extract_text(pdf_path)?;
    let full = pages.join("\n\n");
    // Truncate to ~4000 words for AI context
    let truncated = truncate_to_words(&full, 4000);
    Ok(truncated)
}

/// Get page count using `pdfinfo`.
pub fn get_page_count(pdf_path: &Path) -> Result<u32> {
    let output = Command::new("pdfinfo")
        .arg(pdf_path.to_str().unwrap_or(""))
        .output()
        .context("running pdfinfo (is poppler-utils installed?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("pdfinfo failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("Pages:") {
            let count_str = line.trim_start_matches("Pages:").trim();
            return count_str.parse::<u32>().context("parsing page count");
        }
    }

    bail!("pdfinfo did not return a Pages: line")
}

fn truncate_to_words(text: &str, max_words: usize) -> String {
    let mut word_count = 0;
    let mut byte_pos = 0;
    for word in text.split_whitespace() {
        word_count += 1;
        // find position after this word
        if let Some(pos) = text[byte_pos..].find(word) {
            byte_pos += pos + word.len();
        }
        if word_count >= max_words {
            return text[..byte_pos].to_string();
        }
    }
    text.to_string()
}

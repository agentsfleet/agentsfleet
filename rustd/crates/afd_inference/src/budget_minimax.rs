//! A pinned native tokenizer; unlisted model names retain the byte bound.

use crate::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::{collections::BTreeMap, error::Error as StdError, io::Read as _, sync::LazyLock};
use tiktoken_rs::CoreBPE;
use unicode_normalization::UnicodeNormalization as _;

const VOCABULARY: &[u8] = include_bytes!("../vendor/minimax-m3p1-flash/vocab.tiktoken.gz");
const METADATA: &str = include_str!("../vendor/minimax-m3p1-flash/metadata.json");
const MODELS: &[&str] = &[
    "accounts/fireworks/models/minimax-m3p1-flash",
    "minimax/minimax-m3.1-flash:free",
    "MiniMaxAI/MiniMax-M3.1-Flash",
    "MiniMax-M3.1-Flash",
];

static TOKENIZER: LazyLock<Result<CoreBPE, Box<dyn StdError + Send + Sync>>> =
    LazyLock::new(tokenizer);

#[derive(Deserialize)]
struct Metadata {
    pattern: String,
    special_tokens: BTreeMap<String, u32>,
}

pub(super) fn input_tokens(model: &str, text: &str) -> Option<usize> {
    if !MODELS.contains(&model) {
        return None;
    }
    // An unusable bundled asset keeps the conservative byte bound.
    TOKENIZER.as_ref().ok().map(|tokenizer| {
        let normalized: String = text.nfc().collect();
        tokenizer.encode_with_special_tokens(&normalized).len()
    })
}

fn tokenizer() -> Result<CoreBPE, Box<dyn StdError + Send + Sync>> {
    let vocabulary = vocabulary()?
        .lines()
        .map(decode_token)
        .collect::<Result<_, _>>()?;
    let metadata: Metadata = serde_json::from_str(METADATA)?;
    let special_tokens = metadata.special_tokens.into_iter().collect();
    Ok(CoreBPE::new(vocabulary, special_tokens, &metadata.pattern)?)
}

fn vocabulary() -> Result<String, Box<dyn StdError + Send + Sync>> {
    let mut text = String::new();
    GzDecoder::new(VOCABULARY).read_to_string(&mut text)?;
    Ok(text)
}

fn decode_token(line: &str) -> Result<(Vec<u8>, u32), Box<dyn StdError + Send + Sync>> {
    let (bytes, rank) = line
        .split_once(' ')
        .ok_or_else(|| std::io::Error::other("invalid bundled tokenizer row"))?;
    Ok((STANDARD.decode(bytes)?, rank.parse()?))
}

#[cfg(test)]
#[path = "budget_minimax_tests.rs"]
mod tests;

//! Expected IDs come from the pinned upstream tokenizer through Hugging Face.

use unicode_normalization::UnicodeNormalization as _;

// Value: protects=the vocabulary, normalization, splitting and added-token
// semantics match the published tokenizer; seam=the real native BPE encoder.
#[test]
fn pinned_tokenizer_matches_native_ids_across_languages_and_special_tokens()
-> crate::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tokenizer = super::tokenizer()?;
    for (text, ids) in [
        ("hello world", vec![37964, 2035]),
        ("🌿", vec![79213, 191]),
        ("語 and plain text", vec![7047, 306, 16896, 2943]),
        ("Cafe\u{301} and Café", vec![67, 154_797, 306, 63679]),
        (
            "<tool_call> {\"name\":\"propose_setup\"} </tool_call>",
            vec![
                200_052, 18396, 1906, 15368, 807, 3660, 81461, 22089, 32, 200_053,
            ],
        ),
        (
            "你好，世界！ Русский العربية हिन्दी",
            vec![
                56658, 69511, 1275, 147_175, 19166, 34728, 28497, 68047, 97224, 17552,
            ],
        ),
    ] {
        assert_eq!(
            tokenizer.encode_with_special_tokens(&text.nfc().collect::<String>()),
            ids,
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn every_published_merge_rank_and_added_token_stays_unique()
-> crate::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rows = super::vocabulary()?
        .lines()
        .map(super::decode_token)
        .collect::<crate::Result<Vec<_>, _>>()?;
    assert_eq!(rows.len(), 200_000);
    let unique: std::collections::HashSet<_> = rows.iter().map(|(bytes, _)| bytes).collect();
    assert_eq!(unique.len(), rows.len());
    for (rank, (_, actual)) in rows.iter().enumerate() {
        assert_eq!(usize::try_from(*actual)?, rank);
    }
    assert_eq!(super::tokenizer()?.special_tokens().len(), 61);
    Ok(())
}

//! A message's media reaches the model as its reading, or as a note naming
//! it: never silently dropped.

use std::sync::Arc;

use super::{Media, MediaKind, MediaReader, Part, Unread, Unreadable, render};

fn voice_note() -> Media {
    Media {
        kind: MediaKind::Audio,
        media_type: "audio/ogg".to_owned(),
        name: Some("standup.ogg".to_owned()),
        bytes: Arc::from(vec![0_u8; 2048]),
    }
}

/// A reader that transcribes audio and refuses everything else.
struct Transcriber;

impl MediaReader for Transcriber {
    fn read(&self, media: &Media) -> impl Future<Output = Result<String, Unreadable>> + Send {
        std::future::ready(match media.kind {
            MediaKind::Audio => Ok("Ship the staging fix first.".to_owned()),
            MediaKind::Image | MediaKind::Video | MediaKind::Document => {
                Err(Unreadable::Failed("no OCR here".to_owned()))
            }
        })
    }
}

#[tokio::test]
async fn a_voice_note_reaches_the_model_as_its_transcript() {
    let parts = [
        Part::Text("Listen to this:".to_owned()),
        Part::Media(voice_note()),
    ];
    assert_eq!(
        render(&parts, &Transcriber).await,
        "Listen to this:\n\nShip the staging fix first."
    );
}

#[tokio::test]
async fn an_embedder_that_reads_no_media_names_each_part_instead_of_dropping_it() {
    let pdf = Media {
        kind: MediaKind::Document,
        media_type: "application/pdf".to_owned(),
        name: None,
        bytes: Arc::from(vec![1_u8; 10]),
    };
    let parts = [Part::Media(voice_note()), Part::Media(pdf)];
    assert_eq!(
        render(&parts, &Unread).await,
        "[audio \"standup.ogg\", 2048 bytes: this run cannot read audio content]\n\n\
         [document, 10 bytes: this run cannot read document content]"
    );
}

#[tokio::test]
async fn a_reader_that_fails_says_why() {
    let screenshot = Media {
        kind: MediaKind::Image,
        media_type: "image/png".to_owned(),
        name: Some("error.png".to_owned()),
        bytes: Arc::from(vec![2_u8; 3]),
    };
    assert_eq!(
        render(&[Part::Media(screenshot)], &Transcriber).await,
        "[image \"error.png\", 3 bytes: it could not be read: no OCR here]"
    );
}

#[test]
fn a_media_part_debugs_as_its_size_not_its_bytes() {
    let shown = format!("{:?}", voice_note());
    assert!(shown.contains("bytes: 2048"), "{shown}");
    assert!(!shown.contains("[0, 0"), "{shown}");
}

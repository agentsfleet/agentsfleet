//! Inspect initialized secret bytes immediately before their allocation is freed.
//!
//! This binary alone installs the observer; production keeps its own allocator.

#![expect(clippy::expect_used, reason = "fixture decoding must fail this proof")]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    ptr,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering},
};

struct Watching;

#[global_allocator]
static ALLOCATOR: Watching = Watching;
static WATCHED: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());
static LENGTH: AtomicUsize = AtomicUsize::new(0);
static SEEN: AtomicBool = AtomicBool::new(false);
static WIPED: AtomicBool = AtomicBool::new(false);

// SAFETY: allocation and deallocation delegate to System unchanged. The only
// observation reads previously initialized bytes before System frees them;
// atomics and byte comparisons neither allocate nor unwind.
unsafe impl GlobalAlloc for Watching {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies exactly the layout System requires.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if WATCHED
            .compare_exchange(ptr, ptr::null_mut(), Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let length = LENGTH.load(Ordering::Relaxed);
            if length <= layout.size() {
                // SAFETY: arm records this live buffer's initialized prefix.
                // The matching allocation is still live until System.dealloc
                // below; no other code accesses these bytes during deallocation.
                let bytes = unsafe { std::slice::from_raw_parts(ptr, length) };
                WIPED.store(bytes.iter().all(|byte| *byte == 0), Ordering::Release);
            }
            SEEN.store(true, Ordering::Release);
        }
        // SAFETY: the caller's pointer and layout came from this allocator,
        // which delegates every allocation to System with the same layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

fn arm(bytes: &[u8]) {
    assert!(!bytes.is_empty(), "a wiping proof needs initialized bytes");
    assert!(WATCHED.load(Ordering::Acquire).is_null());
    SEEN.store(false, Ordering::Relaxed);
    WIPED.store(false, Ordering::Relaxed);
    LENGTH.store(bytes.len(), Ordering::Relaxed);
    WATCHED.store(bytes.as_ptr().cast_mut(), Ordering::Release);
}

#[test]
fn owned_secret_buffers_are_wiped_before_free() {
    // One test owns the observer, so parallel tests cannot replace its pointer.
    unwiped_control();
    private_preview();
    daemon_secret_text_and_clone();
    daemon_secret_text_during_unwinding();
    daemon_secret_bytes();
    runner_secret_and_clone();
    replaced_nested_secret();
    dropped_secret_object();
}

fn assert_wiped() {
    assert!(SEEN.load(Ordering::Acquire), "secret allocation was freed");
    assert!(
        WIPED.load(Ordering::Acquire),
        "private bytes were wiped first"
    );
}

fn unwiped_control() {
    let plain = std::hint::black_box(String::from("unwiped negative control"));
    arm(plain.as_bytes());
    drop(plain);
    assert!(SEEN.load(Ordering::Acquire), "control allocation was freed");
    assert!(
        !WIPED.load(Ordering::Acquire),
        "control bytes were not wiped"
    );
}

fn private_preview() {
    let frame: afd_authoring::Delta = serde_json::from_value(serde_json::json!({
        "draft_id":"01990000-0000-7000-8000-000000000001",
        "sequence":"1","stream_id":"01990000-0000-7000-8000-000000000002",
        "stream_seq":1,"stream_start":true,"text_kind":"answer",
        "text":"private preview answer 日本語"
    }))
    .expect("public preview wire shape");
    arm(frame.text.as_bytes());
    drop(frame);
    assert_wiped();
}

fn daemon_secret_text_and_clone() {
    let secret: afd_crypto::secret::SecretString =
        serde_json::from_str("\"private credential 日本語\"").expect("secret text");
    let cloned = secret.clone();
    assert_ne!(secret.expose().as_ptr(), cloned.expose().as_ptr());
    arm(cloned.expose().as_bytes());
    drop(cloned);
    assert_wiped();
    assert_eq!(secret.expose(), "private credential 日本語");
    arm(secret.expose().as_bytes());
    drop(secret);
    assert_wiped();
}

fn daemon_secret_text_during_unwinding() {
    let secret = afd_crypto::secret::SecretString::new("unwinding credential".to_owned());
    arm(secret.expose().as_bytes());
    let outcome = std::panic::catch_unwind(move || {
        let _held = secret;
        std::panic::resume_unwind(Box::new("exercise secret cleanup on unwind"));
    });
    assert!(outcome.is_err(), "the owner unwound");
    assert_wiped();
}

fn daemon_secret_bytes() {
    let secret = afd_crypto::secret::SecretBytes::new(b"decrypted credential bytes".to_vec());
    arm(secret.expose());
    drop(secret);
    assert_wiped();
}

fn runner_secret_and_clone() {
    let secret = afr_secrets::Secret::new("runner credential 日本語".to_owned());
    let cloned = secret.clone();
    assert_ne!(secret.expose().as_ptr(), cloned.expose().as_ptr());
    arm(cloned.expose().as_bytes());
    drop(cloned);
    assert_wiped();
    assert_eq!(secret.expose(), "runner credential 日本語");
    arm(secret.expose().as_bytes());
    drop(secret);
    assert_wiped();
}

fn secret_object() -> afd_crypto::secret::SecretObject {
    afd_crypto::secret::SecretObject::parse(
        br#"{"credential":{"private-field":["nested credential"]},"account":"kept"}"#,
    )
    .expect("nested secret object")
}

fn replaced_nested_secret() {
    let mut object = secret_object();
    let old = object
        .fields()
        .get("credential")
        .and_then(|value| value.pointer("/private-field/0"))
        .and_then(serde_json::Value::as_str)
        .expect("nested credential");
    arm(old.as_bytes());
    object.replace_string("credential", "rotated credential");
    assert_wiped();
    assert_eq!(object.fields()["account"], "kept");
    assert_eq!(object.fields()["credential"], "rotated credential");
    let canonical = object.canonical().expect("canonical secret object");
    arm(canonical.expose());
    drop(canonical);
    assert_wiped();
}

fn dropped_secret_object() {
    for selected in ["nested-value", "nested-key", "root-key"] {
        let object = secret_object();
        let nested = object.fields()["credential"]
            .as_object()
            .expect("nested map");
        let bytes = match selected {
            "nested-value" => nested
                .get("private-field")
                .and_then(|value| value.get(0))
                .and_then(serde_json::Value::as_str)
                .expect("secret")
                .as_bytes(),
            "nested-key" => nested
                .get_key_value("private-field")
                .expect("field")
                .0
                .as_bytes(),
            _ => object
                .fields()
                .get_key_value("credential")
                .expect("field")
                .0
                .as_bytes(),
        };
        arm(bytes);
        drop(object);
        assert_wiped();
    }
}

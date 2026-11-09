use tokio_quiche::{quiche, settings::QuicSettings};

use crate::{Result, tls::Trust};

pub(super) fn client(trust: &Trust) -> Result<quiche::Config> {
    let tls = trust.h3()?;
    let mut config = quiche::Config::with_boring_ssl_ctx_builder(quiche::PROTOCOL_VERSION, tls)?;
    config.set_application_protos(&[b"h3"])?;
    config.set_initial_max_data(u64::from(crate::CONNECTION_WINDOW));
    config.set_initial_max_stream_data_bidi_local(u64::from(crate::STREAM_WINDOW));
    config.set_initial_max_stream_data_bidi_remote(u64::from(crate::STREAM_WINDOW));
    config.set_initial_max_stream_data_uni(u64::from(crate::HEADER_BYTES));
    config.set_initial_max_streams_bidi(u64::from(crate::STREAMS));
    config.set_initial_max_streams_uni(3);
    config.set_max_stream_window(u64::from(crate::STREAM_WINDOW));
    config.set_max_connection_window(u64::from(crate::CONNECTION_WINDOW));
    config.set_max_idle_timeout(30_000);
    Ok(config)
}

pub(super) fn server() -> QuicSettings {
    let mut settings = QuicSettings::default();
    settings.enable_dgram = false;
    settings.enable_early_data = false;
    settings.initial_max_data = u64::from(crate::CONNECTION_WINDOW);
    settings.initial_max_stream_data_bidi_local = u64::from(crate::STREAM_WINDOW);
    settings.initial_max_stream_data_bidi_remote = u64::from(crate::STREAM_WINDOW);
    settings.initial_max_stream_data_uni = u64::from(crate::HEADER_BYTES);
    settings.initial_max_streams_bidi = u64::from(crate::STREAMS);
    settings.initial_max_streams_uni = 3;
    settings.max_connection_window = u64::from(crate::CONNECTION_WINDOW);
    settings.max_stream_window = u64::from(crate::STREAM_WINDOW);
    settings.max_idle_timeout = Some(std::time::Duration::from_secs(30));
    settings.handshake_timeout = Some(std::time::Duration::from_secs(5));
    settings.listen_backlog = 128;
    settings
}

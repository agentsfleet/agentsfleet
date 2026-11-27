//! `fs/search`, answered on the blocking pool like every file call.

use jsonrpsee_types::Id;

use super::Session;
use af_guest_wire::Search;
use af_guest_wire::error::Result;

impl Session {
    /// Runs a search, its answer cut to the frame this session's starter
    /// chose.
    pub(super) fn search(&mut self, id: Id<'static>, params: Result<Search>) {
        let frame = self.sizes.max_frame_bytes;
        self.on_files(id, params, move |workspace, search: Search| {
            workspace.search(&search, frame)
        });
    }
}

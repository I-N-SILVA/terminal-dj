use crate::metadata::TrackMeta;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Library {
    pub tracks: Vec<PathBuf>,
    pub metadata: HashMap<PathBuf, TrackMeta>,
}

impl Library {
    pub fn new() -> Library {
        Library {
            tracks: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    // Legacy synchronous scanner removed in favor of the incremental
    // background scanner implemented in App::load_library. Keeping the type
    // around so existing code that references Library still compiles.

    // If a synchronous scan is ever needed again, reintroduce a dedicated
    // function here that uses spawn_blocking and streams results back.
}

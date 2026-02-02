use std::path::Path;

use crate::init::{InitError, run_init};

pub fn execute(root: &Path, prefix: &Path, no_modify_path: bool) -> Result<(), zb_core::Error> {
    run_init(root, prefix, no_modify_path).map_err(|e| match e {
        InitError::Message(msg) => zb_core::Error::StoreCorruption { message: msg },
    })
}

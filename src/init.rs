use crate::io::init_logging;
use anyhow::{Error, Result};
use once_cell::sync::OnceCell;

static INIT: OnceCell<()> = OnceCell::new();

/// Initialize global ffmpeg settings. This also intializes the
/// logging capability and redirect it to `tracing`.
pub fn init() -> Result<()> {
    INIT.get_or_try_init(|| {
        // ffmpeg::init()?;

        // Redirect logging to the Rust `tracing` crate.
        init_logging();

        Ok::<(), Error>(())
    })?;

    Ok(())
}

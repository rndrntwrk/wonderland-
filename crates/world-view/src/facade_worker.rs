//! Bounded transfer into a disposable worker. Scene validation, layout, lighting,
//! expansion, rasterization and compression run in that worker, not the DOM task.
use crate::{FacadeExportOptions, WorldDocument, WorldError, WorldFacadeJob, WorldFacadeOutput};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Write},
    sync::Arc,
};
const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
fn error(e: impl std::fmt::Display) -> WorldError {
    WorldError(format!("facade worker: {e}"))
}

#[derive(Serialize)]
struct BorrowedRequest<'a> {
    schema: u16,
    world: &'a WorldDocument,
    options: FacadeExportOptions,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerRequest {
    schema: u16,
    world: WorldDocument,
    options: FacadeExportOptions,
}
struct BoundedBytes(Vec<u8>);
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_INPUT_BYTES)
        {
            return Err(io::Error::other("facade worker source byte budget"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Only bounded snapshot marshaling occurs on the caller. This deliberately does
/// not construct a renderer, lightmap, layout or job. The receiver revalidates.
pub fn encode_facade_worker_request(
    world: &WorldDocument,
    options: FacadeExportOptions,
) -> Result<Vec<u8>, WorldError> {
    let mut writer = BoundedBytes(Vec::new());
    serde_json::to_writer(
        &mut writer,
        &BorrowedRequest {
            schema: 1,
            world,
            options,
        },
    )
    .map_err(error)?;
    Ok(writer.0)
}
/// Native-equivalent worker execution. It owns no DOM, live GPU ticket, authority
/// or network capability. A browser cancels it by terminating its whole worker.
pub fn execute_facade_worker_request(bytes: &[u8]) -> Result<WorldFacadeOutput, WorldError> {
    if bytes.is_empty() || bytes.len() > MAX_INPUT_BYTES {
        return Err(error("worker source byte budget"));
    }
    let request: WorkerRequest = serde_json::from_slice(bytes).map_err(error)?;
    if request.schema != 1 {
        return Err(error("worker protocol version"));
    }
    let mut job = WorldFacadeJob::new(Arc::new(request.world), request.options)?;
    // The existing 256-region / 16,384-part limits fit below this bounded scan.
    for _ in 0..65_536 {
        if let Some(output) = job.step(128)? {
            return Ok(output);
        }
    }
    job.cancel();
    Err(error("worker step budget"))
}

//! Share one reader and offload batches with at most two outstanding jobs.

use std::{error::Error, sync::Arc};

use tokio::{runtime, task::JoinSet};

mod async_lookup;

fn main() -> Result<(), Box<dyn Error>> {
    let runtime = runtime::Builder::new_current_thread().build()?;
    runtime.block_on(run())
}

async fn run() -> Result<(), Box<dyn Error>> {
    let reader = Arc::new(tokio::task::spawn_blocking(async_lookup::fixture_reader).await??);
    let mut jobs = JoinSet::new();
    let mut batches = async_lookup::fixture_batches().into_iter();

    loop {
        for addresses in batches.by_ref().take(async_lookup::MAX_OUTSTANDING) {
            let reader = Arc::clone(&reader);
            jobs.spawn_blocking(move || async_lookup::check_fixture_batch(&reader, &addresses));
        }

        if jobs.is_empty() {
            break;
        }

        // Observe every started job before returning the first failure.
        let mut failure: Option<Box<dyn Error>> = None;
        while let Some(result) = jobs.join_next().await {
            let result = result
                .map_err(Box::<dyn Error>::from)
                .and_then(|result| result.map_err(Into::into));

            if failure.is_none() {
                failure = result.err();
            }
        }

        if let Some(error) = failure {
            return Err(error);
        }
    }

    Ok(())
}

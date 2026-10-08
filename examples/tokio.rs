//! Share one reader and offload batches with at most two outstanding jobs.

use std::{error::Error, sync::Arc};

use tokio::{runtime, task::JoinSet};

mod support;

fn main() -> Result<(), Box<dyn Error>> {
    let runtime = runtime::Builder::new_current_thread().build()?;
    runtime.block_on(run())
}

async fn run() -> Result<(), Box<dyn Error>> {
    let reader = Arc::new(tokio::task::spawn_blocking(support::reader).await??);
    let mut jobs = JoinSet::new();
    let mut batches = support::batches().into_iter();

    loop {
        for addresses in batches.by_ref().take(2) {
            let reader = Arc::clone(&reader);
            jobs.spawn_blocking(move || support::lookup_batch(&reader, &addresses));
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

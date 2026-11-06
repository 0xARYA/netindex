//! Keep the runtime local while sharing readers with blocking batch workers.

use std::{error::Error, sync::Arc};

use compio::runtime::Runtime;

mod async_lookup;

fn main() -> Result<(), Box<dyn Error>> {
    let runtime = Runtime::new()?;
    runtime.block_on(run(&runtime))
}

async fn run(runtime: &Runtime) -> Result<(), Box<dyn Error>> {
    let reader = Arc::new(
        runtime
            .spawn_blocking(async_lookup::fixture_reader)
            .await??,
    );
    let mut batches = async_lookup::fixture_batches().into_iter();

    loop {
        let mut jobs = Vec::new();
        for addresses in batches.by_ref().take(async_lookup::MAX_OUTSTANDING) {
            let reader = Arc::clone(&reader);
            jobs.push(
                runtime
                    .spawn_blocking(move || async_lookup::check_fixture_batch(&reader, &addresses)),
            );
        }

        if jobs.is_empty() {
            break;
        }

        // Observe every started job before returning the first failure.
        let mut failure: Option<Box<dyn Error>> = None;
        for job in jobs {
            let result = job
                .await
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

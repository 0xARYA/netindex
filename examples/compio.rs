//! Keep the runtime local while sharing readers with blocking batch workers.

use std::{error::Error, sync::Arc};

use compio::runtime::Runtime;

mod support;

fn main() -> Result<(), Box<dyn Error>> {
    let runtime = Runtime::new()?;
    runtime.block_on(run(&runtime))
}

async fn run(runtime: &Runtime) -> Result<(), Box<dyn Error>> {
    let reader = Arc::new(runtime.spawn_blocking(support::reader).await??);
    let mut batches = support::batches().into_iter();

    loop {
        let mut jobs = Vec::new();
        for addresses in batches.by_ref().take(2) {
            let reader = Arc::clone(&reader);
            jobs.push(runtime.spawn_blocking(move || support::lookup_batch(&reader, &addresses)));
        }

        if jobs.is_empty() {
            break;
        }

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

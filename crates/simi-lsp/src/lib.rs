mod position;
mod server;

pub use server::{Backend, run_connection, run_connection_with_backend};

pub fn run_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    run_stdio_with_backend(Backend::new())
}

pub fn run_stdio_with_module_sources<I, N, S>(
    sources: I,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    I: IntoIterator<Item = (N, S)>,
    N: Into<String>,
    S: Into<String>,
{
    run_stdio_with_backend(Backend::with_module_sources(sources))
}

fn run_stdio_with_backend(
    backend: Backend,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (connection, io_threads) = lsp_server::Connection::stdio();
    run_connection_with_backend(connection, backend)?;
    io_threads.join()?;
    Ok(())
}

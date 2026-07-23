mod position;
mod server;

pub use server::{Backend, run_connection};

pub fn run_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (connection, io_threads) = lsp_server::Connection::stdio();
    run_connection(connection)?;
    io_threads.join()?;
    Ok(())
}

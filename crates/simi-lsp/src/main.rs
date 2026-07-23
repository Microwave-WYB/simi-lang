fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let (connection, io_threads) = lsp_server::Connection::stdio();
    simi_lsp::run_connection(connection)?;
    io_threads.join()?;
    Ok(())
}

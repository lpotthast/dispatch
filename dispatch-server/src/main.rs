#[tokio::main]
async fn main() -> rootcause::Result<()> {
    dispatch_server::run().await
}
